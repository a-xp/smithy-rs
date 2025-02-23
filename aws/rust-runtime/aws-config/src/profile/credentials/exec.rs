/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

use std::sync::Arc;

use aws_sdk_sts::operation::AssumeRole;
use aws_sdk_sts::{Config, Credentials};
use aws_types::region::Region;

use super::repr::{self, BaseProvider};

use crate::profile::credentials::ProfileFileError;
use crate::provider_config::ProviderConfig;
use crate::sts;
use crate::web_identity_token::{StaticConfiguration, WebIdentityTokenCredentialsProvider};
use aws_hyper::AwsMiddleware;
use aws_smithy_client::erase::DynConnector;
use aws_types::credentials::{self, CredentialsError, ProvideCredentials};
use aws_types::os_shim_internal::Fs;
use std::fmt::Debug;

#[derive(Debug)]
pub struct AssumeRoleProvider {
    role_arn: String,
    external_id: Option<String>,
    session_name: Option<String>,
}

#[derive(Debug)]
pub struct ClientConfiguration {
    pub(crate) core_client: aws_smithy_client::Client<DynConnector, AwsMiddleware>,
    pub(crate) region: Option<Region>,
}

impl AssumeRoleProvider {
    pub async fn credentials(
        &self,
        input_credentials: Credentials,
        client_config: &ClientConfiguration,
    ) -> credentials::Result {
        let config = Config::builder()
            .credentials_provider(input_credentials)
            .region(client_config.region.clone())
            .build();
        let session_name = &self
            .session_name
            .as_ref()
            .cloned()
            .unwrap_or_else(|| sts::util::default_session_name("assume-role-from-profile"));
        let operation = AssumeRole::builder()
            .role_arn(&self.role_arn)
            .set_external_id(self.external_id.clone())
            .role_session_name(session_name)
            .build()
            .expect("operation is valid")
            .make_operation(&config)
            .await
            .expect("valid operation");
        let assume_role_creds = client_config
            .core_client
            .call(operation)
            .await
            .map_err(CredentialsError::provider_error)?
            .credentials;
        sts::util::into_credentials(assume_role_creds, "AssumeRoleProvider")
    }
}

#[derive(Debug)]
pub(super) struct ProviderChain {
    base: Arc<dyn ProvideCredentials>,
    chain: Vec<AssumeRoleProvider>,
}

impl ProviderChain {
    pub fn base(&self) -> &dyn ProvideCredentials {
        self.base.as_ref()
    }

    pub fn chain(&self) -> &[AssumeRoleProvider] {
        self.chain.as_slice()
    }
}

impl ProviderChain {
    pub fn from_repr(
        fs: Fs,
        connector: &DynConnector,
        region: Option<Region>,
        repr: repr::ProfileChain,
        factory: &named::NamedProviderFactory,
    ) -> Result<Self, ProfileFileError> {
        let base = match repr.base() {
            BaseProvider::NamedSource(name) => {
                factory
                    .provider(name)
                    .ok_or(ProfileFileError::UnknownProvider {
                        name: name.to_string(),
                    })?
            }
            BaseProvider::AccessKey(key) => Arc::new(key.clone()),
            BaseProvider::WebIdentityTokenRole {
                role_arn,
                web_identity_token_file,
                session_name,
            } => {
                let conf = ProviderConfig::empty()
                    .with_http_connector(connector.clone())
                    .with_fs(fs)
                    .with_region(region);
                let provider = WebIdentityTokenCredentialsProvider::builder()
                    .static_configuration(StaticConfiguration {
                        web_identity_token_file: web_identity_token_file.into(),
                        role_arn: role_arn.to_string(),
                        session_name: session_name.map(|sess| sess.to_string()).unwrap_or_else(
                            || sts::util::default_session_name("web-identity-token-profile"),
                        ),
                    })
                    .configure(&conf)
                    .build();
                Arc::new(provider)
            }
        };
        tracing::info!(base = ?repr.base(), "first credentials will be loaded from {:?}", repr.base());
        let chain = repr
            .chain()
            .iter()
            .map(|role_arn| {
                tracing::info!(role_arn = ?role_arn, "which will be used to assume a role");
                AssumeRoleProvider {
                    role_arn: role_arn.role_arn.into(),
                    external_id: role_arn.external_id.map(|id| id.into()),
                    session_name: role_arn.session_name.map(|id| id.into()),
                }
            })
            .collect();
        Ok(ProviderChain { base, chain })
    }
}

pub mod named {
    use std::collections::HashMap;
    use std::sync::Arc;

    use aws_types::credentials::ProvideCredentials;
    use std::borrow::Cow;

    #[derive(Debug)]
    pub struct NamedProviderFactory {
        providers: HashMap<Cow<'static, str>, Arc<dyn ProvideCredentials>>,
    }

    fn lower_cow(mut inp: Cow<str>) -> Cow<str> {
        if !inp.chars().all(|c| c.is_ascii_lowercase()) {
            inp.to_mut().make_ascii_lowercase();
        }
        inp
    }

    impl NamedProviderFactory {
        pub fn new(providers: HashMap<Cow<'static, str>, Arc<dyn ProvideCredentials>>) -> Self {
            let providers = providers
                .into_iter()
                .map(|(k, v)| (lower_cow(k), v))
                .collect();
            Self { providers }
        }

        pub fn provider(&self, name: &str) -> Option<Arc<dyn ProvideCredentials>> {
            self.providers.get(&lower_cow(Cow::Borrowed(name))).cloned()
        }
    }
}

#[cfg(test)]
mod test {
    use crate::profile::credentials::exec::named::NamedProviderFactory;
    use crate::profile::credentials::exec::ProviderChain;
    use crate::profile::credentials::repr::{BaseProvider, ProfileChain};
    use crate::test_case::no_traffic_connector;
    use aws_sdk_sts::Region;
    use aws_types::Credentials;
    use std::collections::HashMap;
    use std::sync::Arc;

    #[test]
    fn providers_case_insensitive() {
        let mut base = HashMap::new();
        base.insert(
            "Environment".into(),
            Arc::new(Credentials::new("key", "secret", None, None, "test")) as _,
        );
        let provider = NamedProviderFactory::new(base);
        assert!(provider.provider("environment").is_some());
        assert!(provider.provider("envIROnment").is_some());
        assert!(provider.provider(" envIROnment").is_none());
        assert!(provider.provider("Environment").is_some());
    }

    #[test]
    fn error_on_unknown_provider() {
        let factory = NamedProviderFactory::new(HashMap::new());
        let chain = ProviderChain::from_repr(
            Default::default(),
            &no_traffic_connector(),
            Some(Region::new("us-east-1")),
            ProfileChain {
                base: BaseProvider::NamedSource("floozle"),
                chain: vec![],
            },
            &factory,
        );
        let err = chain.expect_err("no source by that name");
        assert!(
            format!("{}", err).contains(
                "profile referenced `floozle` provider but that provider is not supported"
            ),
            "`{}` did not match expected error",
            err
        );
    }
}
