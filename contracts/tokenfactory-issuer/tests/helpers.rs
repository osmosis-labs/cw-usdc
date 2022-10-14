use cosmwasm_std::Coin;

use osmosis_std::types::osmosis::tokenfactory::v1beta1::QueryDenomAuthorityMetadataRequest;
use osmosis_testing::{
    cosmrs::proto::{
        cosmos::bank::v1beta1::{MsgSend, MsgSendResponse},
        cosmwasm::wasm::v1::MsgExecuteContractResponse,
    },
    Account, Bank, Module, OsmosisTestApp, Runner, RunnerError, RunnerExecuteResult,
    SigningAccount, TokenFactory, Wasm,
};
use serde::de::DeserializeOwned;
use std::path::PathBuf;
use tokenfactory_issuer::msg::{
    DenomResponse, ExecuteMsg, InstantiateMsg, IsFrozenResponse, OwnerResponse, QueryMsg,
};

pub struct TestEnv {
    pub test_accs: Vec<SigningAccount>,
    pub tokenfactory_issuer: TokenfactoryIssuer,
}

impl TestEnv {
    pub fn new(instantiate_msg: InstantiateMsg, signer_index: usize) -> Result<Self, RunnerError> {
        let app = OsmosisTestApp::new();
        let test_accs = Self::create_default_test_accs(&app);

        let tokenfactory_issuer =
            TokenfactoryIssuer::new(app, &instantiate_msg, &test_accs[signer_index])?;

        Ok(Self {
            test_accs,
            tokenfactory_issuer,
        })
    }

    pub fn create_default_test_accs(app: &OsmosisTestApp) -> Vec<SigningAccount> {
        let test_accs_count: u64 = 10;
        let default_initial_balance = [Coin::new(100_000_000_000, "uosmo")];

        app.init_accounts(&default_initial_balance, test_accs_count)
            .unwrap()
    }

    pub fn app(&self) -> &OsmosisTestApp {
        &self.tokenfactory_issuer.app
    }
    pub fn tokenfactory(&self) -> TokenFactory<'_, OsmosisTestApp> {
        TokenFactory::new(self.app())
    }

    pub fn bank(&self) -> Bank<'_, OsmosisTestApp> {
        Bank::new(self.app())
    }

    pub fn token_admin(&self, denom: &str) -> String {
        self.tokenfactory()
            .query_denom_authority_metadata(&QueryDenomAuthorityMetadataRequest {
                denom: denom.to_string(),
            })
            .unwrap()
            .authority_metadata
            .unwrap()
            .admin
    }

    pub fn send_tokens(
        &self,
        to: String,
        coins: Vec<Coin>,
        signer: &SigningAccount,
    ) -> RunnerExecuteResult<MsgSendResponse> {
        self.app().execute::<MsgSend, MsgSendResponse>(
            MsgSend {
                from_address: signer.address(),
                to_address: to,
                amount: coins
                    .into_iter()
                    .map(
                        |c| osmosis_testing::cosmrs::proto::cosmos::base::v1beta1::Coin {
                            denom: c.denom,
                            amount: c.amount.to_string(),
                        },
                    )
                    .collect(),
            },
            "/cosmos.bank.v1beta1.MsgSend",
            signer,
        )
    }
}

impl Default for TestEnv {
    fn default() -> Self {
        Self::new(
            InstantiateMsg::NewToken {
                subdenom: "uusd".to_string(),
            },
            0,
        )
        .unwrap()
    }
}

#[derive(Debug)]
pub struct TokenfactoryIssuer {
    pub app: OsmosisTestApp,
    pub code_id: u64,
    pub contract_addr: String,
}

impl TokenfactoryIssuer {
    pub fn new(
        app: OsmosisTestApp,
        instantiate_msg: &InstantiateMsg,
        signer: &SigningAccount,
    ) -> Result<Self, RunnerError> {
        let wasm = Wasm::new(&app);
        let token_creation_fee = Coin::new(10000000, "uosmo");

        let code_id = wasm
            .store_code(&Self::get_wasm_byte_code(), None, signer)?
            .data
            .code_id;
        let contract_addr = wasm
            .instantiate(
                code_id,
                &instantiate_msg,
                None,
                None,
                &[token_creation_fee],
                signer,
            )?
            .data
            .address;

        Ok(Self {
            app,
            code_id,
            contract_addr,
        })
    }

    // executes
    pub fn execute(
        &self,
        execute_msg: &ExecuteMsg,
        funds: &[Coin],
        signer: &SigningAccount,
    ) -> RunnerExecuteResult<MsgExecuteContractResponse> {
        let wasm = Wasm::new(&self.app);
        wasm.execute(&self.contract_addr, execute_msg, funds, signer)
    }

    pub fn set_minter(
        &self,
        address: &str,
        allowance: u128,
        signer: &SigningAccount,
    ) -> RunnerExecuteResult<MsgExecuteContractResponse> {
        self.execute(
            &ExecuteMsg::SetMinter {
                address: address.to_string(),
                allowance: allowance.into(),
            },
            &[],
            signer,
        )
    }
    pub fn mint(
        &self,
        address: &str,
        amount: u128,
        signer: &SigningAccount,
    ) -> RunnerExecuteResult<MsgExecuteContractResponse> {
        self.execute(
            &ExecuteMsg::Mint {
                to_address: address.to_string(),
                amount: amount.into(),
            },
            &[],
            signer,
        )
    }

    pub fn set_freezer(
        &self,
        address: &str,
        status: bool,
        signer: &SigningAccount,
    ) -> RunnerExecuteResult<MsgExecuteContractResponse> {
        self.execute(
            &ExecuteMsg::SetFreezer {
                address: address.to_string(),
                status,
            },
            &[],
            signer,
        )
    }

    pub fn freeze(
        &self,
        status: bool,
        signer: &SigningAccount,
    ) -> RunnerExecuteResult<MsgExecuteContractResponse> {
        self.execute(&ExecuteMsg::Freeze { status }, &[], signer)
    }

    // queries
    pub fn query<T>(&self, query_msg: &QueryMsg) -> Result<T, RunnerError>
    where
        T: DeserializeOwned,
    {
        let wasm = Wasm::new(&self.app);
        wasm.query(&self.contract_addr, query_msg)
    }

    pub fn query_denom(&self) -> Result<DenomResponse, RunnerError> {
        self.query(&QueryMsg::Denom {})
    }

    pub fn query_is_frozen(&self) -> Result<IsFrozenResponse, RunnerError> {
        self.query(&QueryMsg::IsFrozen {})
    }
    pub fn query_owner(&self) -> Result<OwnerResponse, RunnerError> {
        self.query(&QueryMsg::Owner {})
    }

    fn get_wasm_byte_code() -> Vec<u8> {
        let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        std::fs::read(
            manifest_path
                .join("..")
                .join("..")
                .join("target")
                .join("wasm32-unknown-unknown")
                .join("release")
                .join("tokenfactory_issuer.wasm"),
        )
        .unwrap()
    }
}
