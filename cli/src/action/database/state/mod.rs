// Copyright 2018-2022 Cargill Incorporated
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Provides scabbard state migration functionality

mod merkle;

use std::io;
use std::io::prelude::*;
use std::path::PathBuf;
use std::str::FromStr;

use clap::ArgMatches;
use scabbard::store::transact::factory::LmdbDatabaseFactory;
use splinter::error::InternalError;
use transact::state::{Committer, Pruner, Reader, StateChange};

use crate::action::database::{
    stores::{new_upgrade_stores, UpgradeStoresWithLmdb},
    ConnectionUri, SplinterEnvironment,
};

use super::{Action, CliError};

#[cfg(any(feature = "postgres", feature = "sqlite"))]
pub use self::merkle::{DieselInTransactionStateTreeStore, DieselStateTreeStore};
pub use self::merkle::{LazyLmdbMerkleState, LmdbStateTreeStore, MerkleState};

/// A source of available trees
pub trait StateTreeStore {
    fn has_tree(&self, circuit_id: &str, service_id: &str) -> Result<bool, InternalError>;
}

pub struct StateMigrateAction;

impl Action for StateMigrateAction {
    fn run(&mut self, arg_matches: Option<&ArgMatches>) -> Result<(), CliError> {
        let state_dir =
            get_state_dir(arg_matches).map_err(|e| CliError::ActionError(format!("{}", e)))?;
        let lmdb_db_factory = LmdbDatabaseFactory::new_state_db_factory(&state_dir, None);

        let args = arg_matches.ok_or(CliError::RequiresArgs)?;
        let mut in_database = args
            .value_of("in")
            .ok_or_else(|| CliError::ActionError("'in' argument is required".to_string()))?;

        let mut out_database = args
            .value_of("out")
            .ok_or_else(|| CliError::ActionError("'out' argument is required".to_string()))?;

        info!(
            "Attempting to migrate scabbard state from {} to {}",
            in_database, out_database
        );

        if !args.is_present("yes") && !args.is_present("dry_run") {
            warn!(
                "Warning: This will purge the data from `--in` and only the current state \
                root is stored, the rest are purged."
            );
            warn!("Are you sure you wish to migrate scabbard state? [y/N]");
            let stdin = io::stdin();
            let line = stdin.lock().lines().next();
            match line {
                Some(Ok(input)) => match input.as_str() {
                    "y" => (),
                    _ => {
                        info!("Migration cancelled");
                        return Ok(());
                    }
                },
                _ => {
                    return Err(CliError::ActionError(
                        "Unable to get prompt response".to_string(),
                    ))
                }
            }
        }

        // used to check for LMDBM regardless of capitalization
        let lower_in_database = in_database.to_string().to_lowercase();
        let lower_out_database = out_database.to_string().to_lowercase();

        // Get the database uri that wil be used for getting the circuit information. If lmdb
        // is the target directory, we need to use the URI for the in database, otherwise the
        // out database is used.
        let database_uri = match (lower_in_database.as_str(), lower_out_database.as_str()) {
            ("lmdb", "lmdb") => {
                return Err(CliError::ActionError(
                    "LMDB to LMDB is not supported".to_string(),
                ))
            }
            (_, "lmdb") => {
                out_database = lower_out_database.as_str();
                in_database.to_string()
            }
            ("lmdb", _) => {
                in_database = lower_in_database.as_str();
                out_database.to_string()
            }
            (_, _) => {
                return Err(CliError::ActionError(
                    "Command only supports moving state to or from LMDB".to_string(),
                ))
            }
        };

        let in_upgrade_stores = match in_database {
            "lmdb" => {
                let upgrade_stores = new_upgrade_stores(&ConnectionUri::from_str(&database_uri)?)
                    .map_err(|e| {
                    CliError::ActionError(format!(
                        "Unable to get stores to fetch circuit information {}",
                        e
                    ))
                })?;
                Box::new(UpgradeStoresWithLmdb::new(
                    upgrade_stores,
                    lmdb_db_factory.clone(),
                ))
            }
            _ => new_upgrade_stores(&ConnectionUri::from_str(in_database)?).map_err(|e| {
                CliError::ActionError(format!(
                    "Unable to get stores for `--in` database {}: {}",
                    in_database, e
                ))
            })?,
        };

        let out_upgrade_stores = match out_database {
            "lmdb" => {
                let upgrade_stores = new_upgrade_stores(&ConnectionUri::from_str(&database_uri)?)
                    .map_err(|e| {
                    CliError::ActionError(format!(
                        "Unable to get stores to fetch circuit information {}",
                        e
                    ))
                })?;
                Box::new(UpgradeStoresWithLmdb::new(upgrade_stores, lmdb_db_factory))
            }
            _ => new_upgrade_stores(&ConnectionUri::from_str(out_database)?).map_err(|e| {
                CliError::ActionError(format!(
                    "Unable to get stores for `--out` database {}: {}",
                    out_database, e
                ))
            })?,
        };

        // Get the database that will be used to get circuit information
        let upgrade_stores =
            new_upgrade_stores(&ConnectionUri::from_str(&database_uri)?).map_err(|e| {
                CliError::ActionError(format!(
                    "Unable to get stores to fetch circuit information {}",
                    e
                ))
            })?;

        let node_id = if let Some(node_id) = upgrade_stores
            .new_node_id_store()
            .get_node_id()
            .map_err(|e| CliError::ActionError(format!("{}", e)))?
        {
            node_id
        } else {
            // This node has not even set a node id, so it cannot have any circuits.
            info!("Skipping scabbard state migrate, no local node ID found");
            return Ok(());
        };

        let circuits = upgrade_stores
            .new_admin_service_store()
            .list_circuits(&[])
            .map_err(|e| CliError::ActionError(format!("{}", e)))?;

        if circuits.len() == 0 {
            info!("Skipping scabbard state migrate, no circuits found");
            Ok(())
        } else {
            let local_services = circuits.into_iter().flat_map(|circuit| {
                circuit
                    .roster()
                    .iter()
                    .filter_map(|svc| {
                        if svc.node_id() == node_id && svc.service_type() == "scabbard" {
                            Some((
                                circuit.circuit_id().to_string(),
                                svc.service_id().to_string(),
                            ))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            });

            for (circuit_id, service_id) in local_services {
                if !args.is_present("dry_run") {
                    info!("Migrating state data for {}::{}", circuit_id, service_id);
                } else {
                    info!(
                        "Checking if state data for {}::{} could be migrated",
                        circuit_id, service_id
                    );
                }

                let commit_hash_store =
                    upgrade_stores.new_commit_hash_store(&circuit_id, &service_id);
                let commit_hash = commit_hash_store
                    .get_current_commit_hash()
                    .map_err(|e| CliError::ActionError(format!("{}", e)))?
                    .ok_or_else(|| {
                        CliError::ActionError(format!(
                            "No commit hash for service {}::{}",
                            circuit_id, service_id,
                        ))
                    })?;

                let state_reader = in_upgrade_stores
                    .get_merkle_state(&circuit_id, &service_id, false)
                    .map_err(|e| CliError::ActionError(e.to_string()))?;

                // check if the tree already exists and error if so unless force is set
                if !args.is_present("force")
                    && out_upgrade_stores
                        .new_state_tree_store()
                        .has_tree(&circuit_id, &service_id)
                        .map_err(|e| CliError::ActionError(e.to_string()))?
                {
                    return Err(CliError::ActionError(format!(
                        "Merkle Tree for {}::{} in {} already exists",
                        circuit_id, service_id, out_database
                    )));
                }

                // If dry_run, do not actually attempt to move the data
                if !args.is_present("dry_run") {
                    out_upgrade_stores
                        .in_transaction(Box::new(|out_upgrade_stores| {
                            let state_writer = out_upgrade_stores.get_merkle_state(
                                &circuit_id,
                                &service_id,
                                true,
                            )?;

                            match copy_state(&state_reader, commit_hash.to_string(), &state_writer)
                            {
                                Ok(()) => {
                                    // delete the existing scabbard state
                                    state_reader
                                        .delete_tree()
                                        .map_err(|e| InternalError::from_source(Box::new(e)))?;
                                }
                                Err(err) => {
                                    // delete the target scabbard state, so that it doesn't exist.
                                    state_writer
                                        .delete_tree()
                                        .map_err(|e| InternalError::from_source(Box::new(e)))?;
                                    return Err(err);
                                }
                            }

                            Ok(())
                        }))
                        .map_err(|e| CliError::ActionError(e.to_string()))?;
                }
            }
            if !args.is_present("dry_run") {
                info!("Scabbard state successfully migrated to {}", out_database);
            } else {
                info!("Dry run was successful for {}", out_database);
            }

            Ok(())
        }
    }
}

/// Gets the path of splinterd's state directory
///
///
/// # Arguments
///
/// * `arg_matches` - an option of clap ['ArgMatches'](https://docs.rs/clap/2.33.3/clap/struct.ArgMatches.html).
///
/// # Returns
///
/// * PathBuf to state_dir if present in arg_matches, otherwise just the default from
/// SplinterEnvironment
fn get_state_dir(arg_matches: Option<&ArgMatches>) -> Result<PathBuf, CliError> {
    if let Some(arg_matches) = arg_matches {
        match arg_matches.value_of("state_dir") {
            Some(state_dir) => {
                let state_dir = PathBuf::from(state_dir.to_string());
                Ok(
                    std::fs::canonicalize(state_dir.as_path())
                        .unwrap_or_else(|_| state_dir.clone()),
                )
            }
            None => Ok(SplinterEnvironment::load().get_state_path()),
        }
    } else {
        Ok(SplinterEnvironment::load().get_state_path())
    }
}

/// Copy existing scabbard state for the current commit hash from state reader MerkleState to
/// state writer MerkleState
///
/// # Arguments
///
/// * `state_reader` - The MerkleState that holds the state that should be moved
/// * `current_commit_hash` - The current state root hash for the in database
/// * `state_writer` - The MerkleState that the state should be moved to
///
/// # Returns
///
/// * Ok if the state was sucessfully copied and results in the correct state root hash, otherwise
/// a CliError is returned
fn copy_state(
    state_reader: &MerkleState,
    current_commit_hash: String,
    state_writer: &MerkleState,
) -> Result<(), InternalError> {
    let state_changes_iter = state_reader
        .filter_iter(&current_commit_hash, None)
        .map_err(|e| {
            InternalError::with_message(format!("Unable to get leaves for commit hash: {}", e))
        })?;

    let mut count = 0;
    let mut last_state_id = state_writer
        .get_state_root()
        .map_err(|e| InternalError::from_source(Box::new(e)))?;
    let mut state_changes = vec![];
    for state_change in state_changes_iter {
        match state_change {
            Ok((key, value)) => {
                state_changes.push(StateChange::Set { key, value });
                count += 1;

                if count > 1000 {
                    last_state_id =
                        write_and_prune_with_cleanup(state_writer, &last_state_id, &state_changes)?;

                    count = 0;
                    state_changes.clear()
                }
            }
            Err(err) => {
                return Err(InternalError::with_message(format!(
                    "Cannot get state change: {}",
                    err
                )))
            }
        }
    }

    last_state_id = write_and_prune_with_cleanup(state_writer, &last_state_id, &state_changes)?;

    if last_state_id != current_commit_hash {
        return Err(InternalError::with_message(format!(
            "Ending commit hash did not match expected {} != {}",
            last_state_id, current_commit_hash
        )));
    }

    Ok(())
}

fn write_and_prune_with_cleanup(
    merkle_state: &MerkleState,
    state_id: &str,
    state_changes: &[StateChange],
) -> Result<String, InternalError> {
    let next_state_id = merkle_state
        .commit(&state_id.to_string(), state_changes)
        .map_err(|e| {
            InternalError::with_message(format!("Unable to commit state changes {}", e))
        })?;
    merkle_state
        .prune(vec![state_id.to_string()])
        .map_err(|e| {
            InternalError::with_message(format!("Unable to purge previous commit hash {}", e))
        })?;

    merkle_state.remove_pruned_entries().map_err(|e| {
        InternalError::with_message(format!("Unable to remove pruned entries {}", e))
    })?;

    Ok(next_state_id)
}
