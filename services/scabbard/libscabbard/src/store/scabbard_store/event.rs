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

use crate::store::scabbard_store::two_phase::event::Event;

#[derive(Debug, PartialEq, Clone)]
pub enum ConsensusEvent {
    TwoPhaseCommit(Event),
}

// A scabbard consensus event that includes the event ID associated with the event
#[derive(Debug, PartialEq, Clone)]
pub enum IdentifiedConsensusEvent {
    Scabbard2pcConsensusEvent(i64, Event),
}

impl IdentifiedConsensusEvent {
    pub fn deconstruct(self) -> (i64, ConsensusEvent) {
        match self {
            Self::Scabbard2pcConsensusEvent(id, event) => {
                (id, ConsensusEvent::Scabbard2pcConsensusEvent(event))
            }
        }
    }
}
