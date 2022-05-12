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

//! Consensus components to work with Augrim

#[cfg(feature = "scabbardv3-consensus-action-runner")]
pub mod consensus_action_runner;
#[cfg(feature = "scabbardv3-consensus-runner")]
mod consensus_runner;
mod process;
mod value;

#[cfg(feature = "scabbardv3-consensus-action-runner")]
pub use consensus_action_runner::ConsensusActionRunner;
pub use process::ScabbardProcess;
pub use value::ScabbardValue;
