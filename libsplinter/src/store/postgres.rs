// Copyright 2018-2020 Cargill Incorporated
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

use diesel::{
    pg::PgConnection,
    r2d2::{ConnectionManager, Pool},
};

use super::StoreFactory;

/// A `StoryFactory` backed by a PostgreSQL database.
pub struct PgStoreFactory {
    pool: Pool<ConnectionManager<PgConnection>>,
}

impl PgStoreFactory {
    pub fn new(pool: Pool<ConnectionManager<PgConnection>>) -> Self {
        Self { pool }
    }
}

impl StoreFactory for PgStoreFactory {
    #[cfg(feature = "biome-credentials")]
    fn get_biome_credentials_store(&self) -> Box<dyn crate::biome::CredentialsStore> {
        Box::new(crate::biome::DieselCredentialsStore::new(self.pool.clone()))
    }

    #[cfg(feature = "biome-key-management")]
    fn get_biome_key_store(&self) -> Box<dyn crate::biome::KeyStore> {
        Box::new(crate::biome::DieselKeyStore::new(self.pool.clone()))
    }

    #[cfg(feature = "biome-credentials")]
    fn get_biome_refresh_token_store(&self) -> Box<dyn crate::biome::RefreshTokenStore> {
        Box::new(crate::biome::DieselRefreshTokenStore::new(
            self.pool.clone(),
        ))
    }

    fn get_biome_user_store(&self) -> Box<dyn crate::biome::UserStore> {
        Box::new(crate::biome::DieselUserStore::new(self.pool.clone()))
    }

    #[cfg(feature = "biome-oauth-user-store-postgres")]
    fn get_biome_oauth_user_store(&self) -> Box<dyn crate::biome::OAuthUserStore> {
        Box::new(crate::biome::DieselOAuthUserStore::new(self.pool.clone()))
    }

    #[cfg(all(feature = "biome-oauth", not(feature = "postgres")))]
    fn get_biome_oauth_user_store(&self) -> Box<dyn crate::biome::OAuthUserStore> {
        // This configuration cannot be reached within this implementation as the whole struct is
        // guarded by "postgres". It merely satisfies the compiler.
        unreachable!()
    }
}
