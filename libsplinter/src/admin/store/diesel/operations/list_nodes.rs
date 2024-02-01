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

//! Provides the "list nodes" operation for the `DieselAdminServiceStore`.

use std::collections::HashMap;

use diesel::{
    prelude::*,
    sql_types::{Binary, Integer, Nullable, Text},
};

use crate::admin::store::{
    diesel::{
        models::{CircuitMemberModel, NodeEndpointModel},
        schema::{circuit_member, node_endpoint},
    },
    error::AdminServiceStoreError,
    CircuitNode, CircuitNodeBuilder,
};
use crate::error::InvalidStateError;
use crate::public_key::PublicKey;

use super::AdminServiceStoreOperations;

pub(in crate::admin::store::diesel) trait AdminServiceStoreListNodesOperation {
    fn list_nodes(
        &self,
    ) -> Result<Box<dyn ExactSizeIterator<Item = CircuitNode>>, AdminServiceStoreError>;
}

impl<'a, C> AdminServiceStoreListNodesOperation for AdminServiceStoreOperations<'a, C>
where
    C: diesel::Connection,
    String: diesel::deserialize::FromSql<diesel::sql_types::Text, C::Backend>,
    i64: diesel::deserialize::FromSql<diesel::sql_types::BigInt, C::Backend>,
    i32: diesel::deserialize::FromSql<Integer, C::Backend>,
    NodeEndpointModel: diesel::Queryable<(Text, Text), C::Backend>,
    CircuitMemberModel: diesel::Queryable<(Text, Text, Integer, Nullable<Binary>), C::Backend>,
{
    fn list_nodes(
        &self,
    ) -> Result<Box<dyn ExactSizeIterator<Item = CircuitNode>>, AdminServiceStoreError> {
        // Collect all pertinent node entries from the database, including the `circuit_member`
        // and the `node_endpoint`.
        let nodes_info: Vec<(CircuitMemberModel, NodeEndpointModel)> = circuit_member::table
            // As `circuit_member` and `node_endpoint` have a one-to-many relationship, this join
            // will return all matching entries as there are `node_endpoint` entries.
            .order(circuit_member::position)
            .inner_join(node_endpoint::table.on(circuit_member::node_id.eq(node_endpoint::node_id)))
            .load(self.conn)?;
        let mut node_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut nodes: HashMap<String, CircuitMemberModel> = HashMap::new();
        // Iterate over the list of node data retrieved from the database, in order to collect all
        // endpoints associated with the `node_ids` in a HashMap.
        nodes_info.into_iter().for_each(|(node, node_endpoint)| {
            if let Some(endpoint_list) = node_map.get_mut(&node.node_id) {
                endpoint_list.push(node_endpoint.endpoint);
                // Ensure only unique endpoints are added to the node's endpoint list
                endpoint_list.sort();
                endpoint_list.dedup();
            } else {
                node_map.insert(node.node_id.to_string(), vec![node_endpoint.endpoint]);
            }

            if !nodes.contains_key(&node.node_id) {
                nodes.insert(node.node_id.to_string(), node);
            }
        });

        let mut nodes_vec: Vec<CircuitMemberModel> = nodes.into_values().collect();
        nodes_vec.sort_by_key(|node| node.position);

        let nodes: Vec<CircuitNode> = nodes_vec
            .iter()
            .map(|node| {
                let mut builder = CircuitNodeBuilder::new().with_node_id(&node.node_id);

                if let Some(public_key) = &node.public_key {
                    builder = builder.with_public_key(&PublicKey::from_bytes(public_key.to_vec()));
                }

                if let Some(endpoints) = node_map.get(&node.node_id) {
                    builder = builder.with_endpoints(endpoints);
                }

                builder.build()
            })
            .collect::<Result<Vec<CircuitNode>, InvalidStateError>>()
            .map_err(AdminServiceStoreError::InvalidStateError)?;

        Ok(Box::new(nodes.into_iter()))
    }
}
