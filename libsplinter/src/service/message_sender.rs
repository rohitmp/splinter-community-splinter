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

//! Contains `MessageSender` trait.

use crate::error::InternalError;

#[cfg(any(feature = "service-timer-handler", feature = "service-message-handler"))]
use super::MessageConverter;
use super::ServiceId;

/// Sends a message between services on the same circuit.
///
/// Implementations of `MessageSender` takes one generic for the type of message being sent.
pub trait MessageSender<M> {
    /// Send a message of type `M` to a provided service ID
    ///
    /// # Arguments
    ///
    /// * `to_service` - The service ID for the recipient of this message
    /// * `message` - The message to be sent
    fn send(&self, to_service: &ServiceId, message: M) -> Result<(), InternalError>;
}

#[cfg(any(feature = "service-timer-handler", feature = "service-message-handler"))]
pub(super) struct IntoMessageSender<'s, 'c, L, R> {
    inner: &'s dyn MessageSender<R>,
    converter: &'c dyn MessageConverter<L, R>,
    _left: std::marker::PhantomData<L>,
}

#[cfg(any(feature = "service-timer-handler", feature = "service-message-handler"))]
impl<'s, 'c, L, R> IntoMessageSender<'s, 'c, L, R> {
    pub(super) fn new(
        inner: &'s dyn MessageSender<R>,
        converter: &'c dyn MessageConverter<L, R>,
    ) -> Self {
        Self {
            inner,
            converter,
            _left: std::marker::PhantomData,
        }
    }
}

#[cfg(any(feature = "service-timer-handler", feature = "service-message-handler"))]
impl<'s, 'c, L, R> MessageSender<L> for IntoMessageSender<'s, 'c, L, R> {
    fn send(&self, to_service: &ServiceId, message: L) -> Result<(), InternalError> {
        self.inner
            .send(to_service, self.converter.to_right(message)?)
    }
}
