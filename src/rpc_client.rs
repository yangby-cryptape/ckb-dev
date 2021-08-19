use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use ckb_jsonrpc_types as rpc;
use jsonrpc_core::response::Output;
use reqwest::{blocking::Client as InnerClient, Url};

use crate::error::{Error, Result};

#[derive(Debug)]
struct IdGenerator {
    state: AtomicU64,
}

impl Default for IdGenerator {
    fn default() -> Self {
        IdGenerator {
            state: AtomicU64::new(1),
        }
    }
}

impl IdGenerator {
    fn new() -> IdGenerator {
        IdGenerator::default()
    }

    fn next(&self) -> u64 {
        self.state.fetch_add(1, Ordering::SeqCst)
    }
}

macro_rules! jsonrpc {
    (
        $(#[$struct_attr:meta])*
        trait $struct_name:ident {$(
            $(#[$attr:meta])*
            fn $method:ident(&$self:ident $(, $arg_name:ident: $arg_ty:ty)*)
                -> $return_ty:ty;
        )*}
    ) => (
        $(#[$struct_attr])*
        struct $struct_name {
            client: InnerClient,
            url: Url,
            id_generator: IdGenerator,
        }

        impl $struct_name {
            fn new(url: &Url) -> Result<Self> {
                let url = url.clone();
                let client = InnerClient::builder()
                        .timeout(Duration::from_secs(30))
                        .build()
                        .map_err(|err| {
                            let msg = format!("failed to build rpc client since {}", err);
                            Error::Rpc(msg)
                        })?;
                let id_generator = IdGenerator::new();
                Ok(Self { url, client, id_generator })
            }

            $(
                $(#[$attr])*
                fn $method(&$self $(, $arg_name: $arg_ty)*) -> Result<$return_ty> {
                    let method = String::from(stringify!($method));
                    let params = serialize_parameters!($($arg_name,)*);
                    let id = $self.id_generator.next();

                    let mut req_json = serde_json::Map::new();
                    req_json.insert("id".to_owned(), serde_json::json!(id));
                    req_json.insert("jsonrpc".to_owned(), serde_json::json!("2.0"));
                    req_json.insert("method".to_owned(), serde_json::json!(method));
                    req_json.insert("params".to_owned(), params);

                    let output = $self
                        .client
                        .post($self.url.clone())
                        .json(&req_json)
                        .send()
                        .map_err(|err| {
                            let msg = format!("failed to send request since {}", err);
                            Error::Rpc(msg)
                        })?
                        .json::<Output>()
                        .map_err(|err| {
                            let msg = format!("failed to parse rpc output since {}", err);
                            Error::Rpc(msg)
                        })?;
                    match output {
                        Output::Success(success) => {
                            serde_json::from_value(success.result)
                                .map_err(|err| {
                                    let msg = format!("failed to parse rpc return since {}", err);
                                    Error::Rpc(msg)
                                })
                        },
                        Output::Failure(failure) => {
                            let msg = format!(
                                "failed to call \"{}\" since {}",
                                &method,
                                serde_json::to_string(&failure).expect("rpc failure to string"));
                            Err(Error::Rpc(msg))
                        }
                    }
                }
            )*
        }
    )
}

macro_rules! serialize_parameters {
    () => ( serde_json::Value::Null );
    ($($arg_name:ident,)+) => ( serde_json::to_value(($($arg_name,)+))?)
}

jsonrpc!(
    trait Client {
        fn get_peers(&self) -> Vec<rpc::RemoteNode>;
    }
);

pub(crate) struct RpcClient {
    inner: Client,
}

impl RpcClient {
    pub(crate) fn new(url: &Url) -> Result<Self> {
        let inner = Client::new(url)?;
        Ok(Self { inner })
    }

    pub(crate) fn get_peers(&self) -> Result<Vec<rpc::RemoteNode>> {
        self.inner.get_peers()
    }
}
