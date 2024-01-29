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

//! Builder for the NetworkSubsystem

use std::sync::{Arc, Mutex};
use std::time::Duration;

use splinter::error::InternalError;
use splinter::transport::multi::MultiTransport;
use splinter::transport::socket::TcpTransport;

use crate::node::runnable::network::RunnableNetworkSubsystem;

const DEFAULT_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Default)]
pub struct NetworkSubsystemBuilder {
    node_id: Option<String>,
    heartbeat_interval: Option<Duration>,
    strict_ref_counts: bool,
    network_endpoints: Option<Vec<String>>,
    signing_context: Option<Arc<Mutex<Box<dyn cylinder::VerifierFactory>>>>,
    signers: Option<Vec<Box<dyn cylinder::Signer>>>,
}

impl NetworkSubsystemBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Specifies the id for the node. Defaults to a random node id.
    pub fn with_node_id(mut self, node_id: String) -> Self {
        self.node_id = Some(node_id);
        self
    }

    /// Specifies the heartbeat interval between peer connections. Defaults to 30 seconds.
    pub fn with_heartbeat_interval(mut self, heartbeat_interval: Duration) -> Self {
        self.heartbeat_interval = Some(heartbeat_interval);
        self
    }

    /// Configure whether or not strict reference counts will be used in the peer manager. Defaults
    /// to false.
    pub fn with_strict_ref_counts(mut self, strict_ref_counts: bool) -> Self {
        self.strict_ref_counts = strict_ref_counts;
        self
    }

    /// Specifies the network endpoints for the node
    pub fn with_network_endpoints(mut self, network_endpoints: Vec<String>) -> Self {
        self.network_endpoints = Some(network_endpoints);
        self
    }

    /// Specifies the signing context for the node
    pub fn with_signing_context(
        mut self,
        signing_context: Arc<Mutex<Box<dyn cylinder::VerifierFactory>>>,
    ) -> Self {
        self.signing_context = Some(signing_context);
        self
    }

    /// Specifies the signers for the node to use in challenge_authorization
    pub fn with_signers(mut self, signers: Vec<Box<dyn cylinder::Signer>>) -> Self {
        self.signers = Some(signers);
        self
    }

    pub fn build(mut self) -> Result<RunnableNetworkSubsystem, InternalError> {
        let node_id = self.node_id.take().ok_or_else(|| {
            InternalError::with_message(
                "Cannot build NetworkSubsystem without a node id".to_string(),
            )
        })?;

        let signing_context = self.signing_context.take().ok_or_else(|| {
            InternalError::with_message(
                "Cannot build NetworkSubsystem without a signing context".to_string(),
            )
        })?;

        let signers = self.signers.unwrap_or_default();

        // keep as option, if not provided will be set to tcp://127.0.0.1:0
        let network_endpoints = self.network_endpoints;

        let heartbeat_interval = self
            .heartbeat_interval
            .take()
            .unwrap_or(DEFAULT_HEARTBEAT_INTERVAL);

        let transport = MultiTransport::new(vec![Box::<TcpTransport>::default()]);

        Ok(RunnableNetworkSubsystem {
            node_id,
            transport,
            heartbeat_interval,
            strict_ref_counts: self.strict_ref_counts,
            network_endpoints,
            signing_context,
            signers,
        })
    }
}
