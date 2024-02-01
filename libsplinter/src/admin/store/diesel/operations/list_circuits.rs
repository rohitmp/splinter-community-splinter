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

//! Provides the "list circuits" operation for the `DieselAdminServiceStore`.

use std::collections::HashMap;
use std::convert::TryFrom;

use diesel::sql_types::{Binary, Integer, Nullable, Text};
use diesel::{dsl::exists, prelude::*};

use crate::admin::store::{
    diesel::{
        models::{
            CircuitMemberModel, CircuitModel, CircuitStatusModel, NodeEndpointModel,
            ServiceArgumentModel, ServiceModel,
        },
        schema::{circuit, circuit_member, node_endpoint, service, service_argument},
    },
    error::AdminServiceStoreError,
    AuthorizationType, Circuit, CircuitBuilder, CircuitNode, CircuitNodeBuilder, CircuitPredicate,
    CircuitStatus, DurabilityType, PersistenceType, RouteType, Service, ServiceBuilder,
};
use crate::error::InvalidStateError;
use crate::public_key::PublicKey;

use super::AdminServiceStoreOperations;

pub(in crate::admin::store::diesel) trait AdminServiceStoreListCircuitsOperation {
    fn list_circuits(
        &self,
        predicates: &[CircuitPredicate],
    ) -> Result<Box<dyn ExactSizeIterator<Item = Circuit>>, AdminServiceStoreError>;
}

impl<'a, C> AdminServiceStoreListCircuitsOperation for AdminServiceStoreOperations<'a, C>
where
    C: diesel::Connection,
    String: diesel::deserialize::FromSql<Text, C::Backend>,
    i64: diesel::deserialize::FromSql<diesel::sql_types::BigInt, C::Backend>,
    i32: diesel::deserialize::FromSql<Integer, C::Backend>,
    i16: diesel::deserialize::FromSql<diesel::sql_types::SmallInt, C::Backend>,
    CircuitMemberModel: diesel::Queryable<(Text, Text, Integer, Nullable<Binary>), C::Backend>,
{
    fn list_circuits(
        &self,
        predicates: &[CircuitPredicate],
    ) -> Result<Box<dyn ExactSizeIterator<Item = Circuit>>, AdminServiceStoreError> {
        // Collect the management types included in the list of `CircuitPredicates`
        let management_types: Vec<String> = predicates
            .iter()
            .filter_map(|pred| match pred {
                CircuitPredicate::ManagementTypeEq(man_type) => Some(man_type.to_string()),
                _ => None,
            })
            .collect::<Vec<String>>();
        // Collects the members included in the list of `CircuitPredicates`
        let members: Vec<String> = predicates
            .iter()
            .filter_map(|pred| match pred {
                CircuitPredicate::MembersInclude(members) => Some(members.to_vec()),
                _ => None,
            })
            .flatten()
            .collect();
        let statuses: Vec<CircuitStatusModel> = predicates
            .iter()
            .filter_map(|pred| match pred {
                CircuitPredicate::CircuitStatus(status) => Some(CircuitStatusModel::from(status)),
                _ => None,
            })
            .collect();
        self.conn
            .transaction::<Box<dyn ExactSizeIterator<Item = Circuit>>, _, _>(|| {
                // Collects circuits which match the circuit predicates
                let mut query = circuit::table.into_boxed().select(circuit::all_columns);

                if !management_types.is_empty() {
                    query = query.filter(circuit::circuit_management_type.eq_any(management_types));
                }

                if !members.is_empty() {
                    query = query.filter(exists(
                        // Selects all `circuit_member` entries where the `node_id` is equal
                        // to any of the members in the circuit predicates
                        circuit_member::table.filter(
                            circuit_member::circuit_id
                                .eq(circuit::circuit_id)
                                .and(circuit_member::node_id.eq_any(members)),
                        ),
                    ));
                }

                if statuses.is_empty() {
                    // By default, only display active circuits
                    query = query.filter(circuit::circuit_status.eq(CircuitStatusModel::Active));
                } else {
                    query = query.filter(
                        // Select only circuits that have the `CircuitStatus` in the predicates
                        circuit::circuit_status.eq_any(statuses),
                    );
                }

                let circuits: Vec<CircuitModel> = query
                    .order(circuit::circuit_id.desc())
                    .load::<CircuitModel>(self.conn)?;

                // Store circuit IDs separately to make it easier to filter following queries
                let circuit_ids: Vec<&str> = circuits
                    .iter()
                    .map(|circuit| circuit.circuit_id.as_str())
                    .collect();

                // Collect the `Circuit` members and put them in a HashMap to associate the list
                // of `node_ids` to the `circuit_id`
                let mut circuit_members: HashMap<String, Vec<CircuitMemberModel>> = HashMap::new();
                let mut node_map: HashMap<String, Vec<String>> = HashMap::new();
                for (member, node_endpoint) in circuit_member::table
                    .filter(circuit_member::circuit_id.eq_any(&circuit_ids))
                    .inner_join(
                        node_endpoint::table.on(circuit_member::node_id.eq(node_endpoint::node_id)),
                    )
                    .load::<(CircuitMemberModel, NodeEndpointModel)>(self.conn)?
                {
                    if let Some(endpoint_list) = node_map.get_mut(&member.node_id) {
                        endpoint_list.push(node_endpoint.endpoint);
                        // Ensure only unique endpoints are added to the node's endpoint list
                        endpoint_list.sort();
                        endpoint_list.dedup();
                    } else {
                        node_map.insert(member.node_id.to_string(), vec![node_endpoint.endpoint]);
                    }

                    if let Some(members) = circuit_members.get_mut(&member.circuit_id) {
                        members.push(member);
                    } else {
                        circuit_members.insert(member.circuit_id.to_string(), vec![member]);
                    }
                }

                // Create HashMap of (`circuit_id`, ` service_id`) to a `ServiceModel`
                let mut services: HashMap<(String, String), ServiceModel> = HashMap::new();
                // Create HashMap of (`circuit_id`, `service_id`) to the associated argument values
                let mut arguments_map: HashMap<(String, String), Vec<ServiceArgumentModel>> =
                    HashMap::new();
                // Collects all `service` and `service_argument` entries using an inner_join on the
                // `service_id`, since the relationship between `service` and `service_argument` is
                // one-to-many. Adding the models retrieved from the database backend to HashMaps
                // removed the duplicate `service` entries collected, and also makes it simpler
                // to build each `Service` later on.
                for (service, opt_arg) in service::table
                    // Filters the services based on the circuit_ids collected based on the circuits
                    // which matched the predicates.
                    .filter(service::circuit_id.eq_any(&circuit_ids))
                    // Joins a `service_argument` entry to a `service` entry, based on `service_id`.
                    .left_join(
                        service_argument::table.on(service::service_id
                            .eq(service_argument::service_id)
                            .and(service_argument::circuit_id.eq(service::circuit_id))),
                    )
                    // Collects all data from the `service` entry, and the pertinent data from the
                    // `service_argument` entry.
                    // Making `service_argument` nullable is required to return all matching
                    // records since the relationship with services is one-to-many for each.
                    .select((
                        service::all_columns,
                        service_argument::all_columns.nullable(),
                    ))
                    .load::<(ServiceModel, Option<ServiceArgumentModel>)>(self.conn)?
                {
                    if let Some(arg_model) = opt_arg {
                        if let Some(args) = arguments_map.get_mut(&(
                            service.circuit_id.to_string(),
                            service.service_id.to_string(),
                        )) {
                            args.push(arg_model);
                        } else {
                            arguments_map.insert(
                                (
                                    service.circuit_id.to_string(),
                                    service.service_id.to_string(),
                                ),
                                vec![arg_model],
                            );
                        }
                    }
                    // Insert new `ServiceBuilder` if it does not already exist
                    services
                        .entry((
                            service.circuit_id.to_string(),
                            service.service_id.to_string(),
                        ))
                        .or_insert_with(|| service);
                }
                // Collect the `Services` mapped to `circuit_ids` after adding any
                // `service_arguments` to the `ServiceBuilder`.
                let mut built_services: HashMap<String, Vec<Service>> = HashMap::new();

                let mut service_vec: Vec<((String, String), ServiceModel)> =
                    services.into_iter().collect();
                service_vec.sort_by_key(|(_, service)| service.position);

                for ((circuit_id, service_id), service) in service_vec.into_iter() {
                    let mut builder = ServiceBuilder::new()
                        .with_service_id(&service.service_id)
                        .with_service_type(&service.service_type)
                        .with_node_id(&service.node_id);

                    if let Some(args) =
                        arguments_map.get_mut(&(circuit_id.to_string(), service_id.to_string()))
                    {
                        args.sort_by_key(|arg| arg.position);
                        builder = builder.with_arguments(
                            &args
                                .iter()
                                .map(|args| (args.key.to_string(), args.value.to_string()))
                                .collect::<Vec<(String, String)>>(),
                        );
                    }
                    let service = builder
                        .build()
                        .map_err(AdminServiceStoreError::InvalidStateError)?;

                    if let Some(service_list) = built_services.get_mut(&circuit_id) {
                        service_list.push(service);
                    } else {
                        built_services.insert(circuit_id.to_string(), vec![service]);
                    }
                }

                let mut ret_circuits: Vec<Circuit> = Vec::new();
                for model in circuits {
                    let mut circuit_builder = CircuitBuilder::new()
                        .with_circuit_id(&model.circuit_id)
                        .with_authorization_type(&AuthorizationType::try_from(
                            model.authorization_type,
                        )?)
                        .with_persistence(&PersistenceType::try_from(model.persistence)?)
                        .with_durability(&DurabilityType::try_from(model.durability)?)
                        .with_routes(&RouteType::try_from(model.routes)?)
                        .with_circuit_management_type(&model.circuit_management_type)
                        .with_circuit_version(model.circuit_version)
                        .with_circuit_status(&CircuitStatus::from(&model.circuit_status));

                    if let Some(display_name) = &model.display_name {
                        circuit_builder = circuit_builder.with_display_name(display_name);
                    }
                    if let Some(members) = circuit_members.get_mut(&model.circuit_id) {
                        members.sort_by_key(|node| node.position);

                        let circuit_node_members: Vec<CircuitNode> = members
                            .iter()
                            .map(|member| {
                                let mut builder =
                                    CircuitNodeBuilder::new().with_node_id(&member.node_id);

                                if let Some(endpoints) = node_map.get(&member.node_id) {
                                    builder = builder.with_endpoints(endpoints);
                                }

                                if let Some(public_key) = &member.public_key {
                                    builder = builder.with_public_key(&PublicKey::from_bytes(
                                        public_key.to_vec(),
                                    ));
                                }

                                builder.build()
                            })
                            .collect::<Result<Vec<CircuitNode>, InvalidStateError>>()
                            .map_err(AdminServiceStoreError::InvalidStateError)?;

                        circuit_builder = circuit_builder.with_members(&circuit_node_members);
                    }
                    if let Some(services) = built_services.get(&model.circuit_id) {
                        circuit_builder = circuit_builder.with_roster(services);
                    }

                    ret_circuits.push(
                        circuit_builder
                            .build()
                            .map_err(AdminServiceStoreError::InvalidStateError)?,
                    );
                }

                Ok(Box::new(ret_circuits.into_iter()))
            })
    }
}
