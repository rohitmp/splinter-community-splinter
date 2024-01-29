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

use std::fmt;

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use crate::action::api::ServerError;
use crate::error::CliError;

use super::{Pageable, RBAC_PROTOCOL_VERSION};

#[derive(Debug, Deserialize, Serialize)]
pub struct Role {
    pub role_id: String,
    pub display_name: String,
    pub permissions: Vec<String>,
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Id: {}", self.role_id)?;
        write!(f, "\n    Name: {}", self.display_name)?;
        f.write_str("\n    Permissions:")?;

        for perm in self.permissions.iter() {
            write!(f, "\n        {}", perm)?;
        }

        Ok(())
    }
}

impl Pageable for Role {
    fn label() -> &'static str {
        "role list"
    }
}

/// Constructs roles for submission to a splinter node.
#[derive(Default)]
pub struct RoleBuilder {
    role_id: Option<String>,
    display_name: Option<String>,
    permissions: Vec<String>,
}

impl RoleBuilder {
    /// Sets the role id of the resulting Role.
    ///
    /// Must not be empty.
    pub fn with_role_id(mut self, role_id: String) -> Self {
        self.role_id = Some(role_id);
        self
    }

    /// Sets the display name of the resulting Role.
    pub fn with_display_name(mut self, display_name: String) -> Self {
        self.display_name = Some(display_name);
        self
    }

    /// Sets the permissions included in the resulting Role.
    ///
    /// Must not be empty.
    pub fn with_permissions(mut self, permissions: Vec<String>) -> Self {
        self.permissions = permissions;
        self
    }

    /// Constructs the Role.
    pub fn build(self) -> Result<Role, CliError> {
        let RoleBuilder {
            role_id,
            display_name,
            permissions,
        } = self;

        if permissions.is_empty() {
            return Err(CliError::ActionError(
                "A role must have at least one permission".into(),
            ));
        }

        let role_id =
            role_id.ok_or_else(|| CliError::ActionError("A role must have a role ID".into()))?;
        if role_id.is_empty() {
            return Err(CliError::ActionError("A role ID must not be blank".into()));
        }

        let display_name = display_name
            .ok_or_else(|| CliError::ActionError("A role must have a display name".into()))?;

        Ok(Role {
            role_id,
            display_name,
            permissions,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct RoleUpdate {
    #[serde(skip)]
    role_id: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    permissions: Option<Vec<String>>,
}

#[derive(Default)]
pub struct RoleUpdateBuilder {
    role_id: Option<String>,
    display_name: Option<String>,
    permissions: Option<Vec<String>>,
}

impl RoleUpdateBuilder {
    /// Sets the role id of the resulting Role.
    ///
    /// Must not be empty.
    pub fn with_role_id(mut self, role_id: String) -> Self {
        self.role_id = Some(role_id);
        self
    }

    /// Sets the display name of the resulting Role.
    pub fn with_display_name(mut self, display_name: Option<String>) -> Self {
        self.display_name = display_name;
        self
    }

    /// Sets the permissions included in the resulting Role.
    ///
    /// Must not be empty.
    pub fn with_permissions(mut self, permissions: Option<Vec<String>>) -> Self {
        self.permissions = permissions;
        self
    }

    /// Constructs the Role.
    pub fn build(self) -> Result<RoleUpdate, CliError> {
        let RoleUpdateBuilder {
            role_id,
            display_name,
            permissions,
        } = self;

        let role_id =
            role_id.ok_or_else(|| CliError::ActionError("A role must have a role ID".into()))?;
        if role_id.is_empty() {
            return Err(CliError::ActionError("A role ID must not be blank".into()));
        }

        if let Some(permissions) = permissions.as_ref() {
            if permissions.is_empty() {
                return Err(CliError::ActionError(
                    "A role must have at least one permission".into(),
                ));
            }
        }

        Ok(RoleUpdate {
            role_id,
            display_name,
            permissions,
        })
    }
}

#[derive(Deserialize)]
struct RoleGet {
    #[serde(rename = "data")]
    role: Role,
}

pub fn get_role(base_url: &str, auth: &str, role_id: &str) -> Result<Option<Role>, CliError> {
    Client::new()
        .get(format!("{}/authorization/roles/{}", base_url, role_id))
        .header("SplinterProtocolVersion", RBAC_PROTOCOL_VERSION)
        .header("Authorization", auth)
        .send()
        .map_err(|err| CliError::ActionError(format!("Failed to fetch role {}: {}", role_id, err)))
        .and_then(|res| {
            let status = res.status();
            if status.is_success() {
                res.json::<RoleGet>()
                    .map_err(|_| {
                        CliError::ActionError(
                            "Request was successful, but received an invalid response".into(),
                        )
                    })
                    .map(|wrapper| Some(wrapper.role))
            } else if status.as_u16() == 401 {
                Err(CliError::ActionError("Not Authorized".into()))
            } else if status.as_u16() == 404 {
                Ok(None)
            } else {
                let message = res
                    .json::<ServerError>()
                    .map_err(|_| {
                        CliError::ActionError(format!(
                            "Get role fetch request failed with status code '{}', but error \
                                 response was not valid",
                            status
                        ))
                    })?
                    .message;

                Err(CliError::ActionError(format!(
                    "Failed to get role {}: {}",
                    role_id, message
                )))
            }
        })
}

pub fn create_role(base_url: &str, auth: &str, role: Role) -> Result<(), CliError> {
    Client::new()
        .post(format!("{}/authorization/roles", base_url))
        .header("SplinterProtocolVersion", RBAC_PROTOCOL_VERSION)
        .header("Authorization", auth)
        .json(&role)
        .send()
        .map_err(|err| CliError::ActionError(format!("Failed to create role: {}", err)))
        .and_then(|res| {
            let status = res.status();
            if status.is_success() {
                Ok(())
            } else if status.as_u16() == 401 {
                Err(CliError::ActionError("Not Authorized".into()))
            } else {
                let message = res
                    .json::<ServerError>()
                    .map_err(|_| {
                        CliError::ActionError(format!(
                            "Create role request failed with status code '{}', but error response \
                            was not valid",
                            status
                        ))
                    })?
                    .message;

                Err(CliError::ActionError(format!(
                    "Failed to create role: {}",
                    message
                )))
            }
        })
}

pub fn update_role(base_url: &str, auth: &str, role_update: RoleUpdate) -> Result<(), CliError> {
    Client::new()
        .patch(format!(
            "{}/authorization/roles/{}",
            base_url, role_update.role_id
        ))
        .header("SplinterProtocolVersion", RBAC_PROTOCOL_VERSION)
        .header("Authorization", auth)
        .json(&role_update)
        .send()
        .map_err(|err| CliError::ActionError(format!("Failed to update role: {}", err)))
        .and_then(|res| {
            let status = res.status();
            if status.is_success() {
                Ok(())
            } else if status.as_u16() == 401 {
                Err(CliError::ActionError("Not Authorized".into()))
            } else if status.as_u16() == 404 {
                Err(CliError::ActionError(format!(
                    "Role {} does not exist",
                    role_update.role_id
                )))
            } else {
                let message = res
                    .json::<ServerError>()
                    .map_err(|_| {
                        CliError::ActionError(format!(
                            "Update role request failed with status code '{}', but error response \
                            was not valid",
                            status
                        ))
                    })?
                    .message;

                Err(CliError::ActionError(format!(
                    "Failed to update role: {}",
                    message
                )))
            }
        })
}

pub fn delete_role(base_url: &str, auth: &str, role_id: &str) -> Result<(), CliError> {
    Client::new()
        .delete(format!("{}/authorization/roles/{}", base_url, role_id))
        .header("SplinterProtocolVersion", RBAC_PROTOCOL_VERSION)
        .header("Authorization", auth)
        .send()
        .map_err(|err| CliError::ActionError(format!("Failed to delete role {}: {}", role_id, err)))
        .and_then(|res| {
            let status = res.status();
            if status.is_success() {
                Ok(())
            } else if status.as_u16() == 401 {
                Err(CliError::ActionError("Not Authorized".into()))
            } else {
                let message = res
                    .json::<ServerError>()
                    .map_err(|_| {
                        CliError::ActionError(format!(
                            "Delete role request failed with status code '{}', but error response \
                            was not valid",
                            status
                        ))
                    })?
                    .message;

                Err(CliError::ActionError(format!(
                    "Failed to delete role {}: {}",
                    role_id, message
                )))
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests the role builder in both Ok and Err scenarios
    /// 1. Construct a valid role
    /// 2. Fail with no role_id
    /// 3. Fail with an empty role_id
    /// 4. Fail with no display name
    /// 4. Succeed with empty display name
    /// 5. Fail with empty permissions
    #[test]
    fn test_role_builder() {
        // Ok Role
        let role = RoleBuilder::default()
            .with_role_id("valid_role".into())
            .with_display_name("Valid Role".into())
            .with_permissions(vec!["a".to_string(), "b".to_string()])
            .build()
            .expect("could not build a valid role");

        assert_eq!("valid_role", &role.role_id);
        assert_eq!("Valid Role", &role.display_name);
        assert_eq!(vec!["a".to_string(), "b".to_string()], role.permissions);

        // Missing role_id
        let res = RoleBuilder::default()
            .with_display_name("No ID Role".into())
            .with_permissions(vec!["a".to_string(), "b".to_string()])
            .build();

        assert!(res.is_err());

        // Empty role_id
        let res = RoleBuilder::default()
            .with_role_id("".into())
            .with_display_name("Empty ID Role".into())
            .with_permissions(vec!["a".to_string(), "b".to_string()])
            .build();
        assert!(res.is_err());

        // No display name
        let res = RoleBuilder::default()
            .with_role_id("no_display_name".into())
            .with_permissions(vec!["a".to_string(), "b".to_string()])
            .build();
        assert!(res.is_err());

        // Empty display name
        RoleBuilder::default()
            .with_role_id("empty_display_name".into())
            .with_display_name("".into())
            .with_permissions(vec!["a".to_string(), "b".to_string()])
            .build()
            .expect("Could not build a role with an empty display name");

        // Empty permissions
        let res = RoleBuilder::default()
            .with_role_id("empty_permissions".into())
            .with_display_name("Empty Permissions".into())
            .with_permissions(vec![])
            .build();
        assert!(res.is_err());
    }

    /// Tests the role update builder in both Ok and Err scenarios
    /// 1. Construct a valid update with all items
    /// 2. Construct a valid update with no permission changes
    /// 3. Construct a valid update with no display name changes
    /// 4. Fail with no role_id
    /// 5. Fail with empty permissions
    #[test]
    fn test_role_update_builder() {
        // Complete valid role update
        let role_update = RoleUpdateBuilder::default()
            .with_role_id("valid_role".into())
            .with_display_name(Some("Valid Role".into()))
            .with_permissions(Some(vec!["a".to_string(), "b".to_string()]))
            .build()
            .expect("could not build a valid role");

        assert_eq!("valid_role", &role_update.role_id);
        assert_eq!(Some("Valid Role"), role_update.display_name.as_deref());
        assert_eq!(
            Some(vec!["a".to_string(), "b".to_string()]),
            role_update.permissions
        );

        // Valid role update with no permission change
        let role_update = RoleUpdateBuilder::default()
            .with_role_id("valid_role".into())
            .with_display_name(Some("Valid Role".into()))
            .build()
            .expect("could not build a valid role");

        assert_eq!("valid_role", &role_update.role_id);
        assert_eq!(Some("Valid Role"), role_update.display_name.as_deref());
        assert_eq!(None, role_update.permissions);

        // Valid role update with no display name
        let role_update = RoleUpdateBuilder::default()
            .with_role_id("valid_role".into())
            .with_permissions(Some(vec!["a".to_string(), "b".to_string()]))
            .build()
            .expect("could not build a valid role");

        assert_eq!("valid_role", &role_update.role_id);
        assert_eq!(None, role_update.display_name);
        assert_eq!(
            Some(vec!["a".to_string(), "b".to_string()]),
            role_update.permissions
        );

        // Missing role_id
        let res = RoleUpdateBuilder::default()
            .with_display_name(Some("No ID Role".into()))
            .with_permissions(Some(vec!["a".to_string(), "b".to_string()]))
            .build();

        assert!(res.is_err());

        // Empty permissions
        let res = RoleUpdateBuilder::default()
            .with_role_id("missing_perms_update".into())
            .with_display_name(Some("Missing Permissions Update".into()))
            .with_permissions(Some(vec![]))
            .build();
        assert!(res.is_err());
    }
}
