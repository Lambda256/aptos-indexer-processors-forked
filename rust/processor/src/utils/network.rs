use std::fmt;

#[derive(Debug)]
pub enum Network {
    Mainnet = 1,
    Testnet = 2,
}

impl Network {
    pub fn from_chain_id(chain_id: u64) -> Option<Self> {
        match chain_id {
            1 => Some(Network::Mainnet),
            2 => Some(Network::Testnet),
            _ => None,
        }
    }
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Network::Mainnet => write!(f, "mainnet"),
            Network::Testnet => write!(f, "testnet"),
        }
    }
}