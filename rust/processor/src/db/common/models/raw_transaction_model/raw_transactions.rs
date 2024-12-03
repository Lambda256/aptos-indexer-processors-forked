
// {
//     "version": 123456789,
//     "hash": "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
//     "state_change_hash": "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
//     "event_root_hash": "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
//     "state_checkpoint_hash": null,
//     "gas_used": 123,
//     "success": true,
//     "vm_status": "Executed successfully",
//     "type": "user_transaction",
//     "timestamp_usecs": 1234567890,
//     "sender": "0x1234567890abcdef1234567890abcdef",
//     "sequence_number": 42,
//     "max_gas_amount": 2000,
//     "gas_unit_price": 100,
//     "expiration_timestamp_secs": 1234567890,
  
//     "signature": {
//       "type": "ed25519_signature",
//       "public_key": "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
//       "signature": "0x567890abcdef567890abcdef567890abcdef567890abcdef567890abcdef5678"
//     },
  
//     "payload": {
//       "type": "entry_function_payload",
//       "function": "0x1::coin::transfer",
//       "type_arguments": ["0x1::aptos_coin::AptosCoin"],
//       "arguments": ["0x234567890abcdef1234567890abcdef", 1000]
//     },
  
//     "changes": [{
//       "address": "0x1234567890abcdef1234567890abcdef",
//       "state_key_hash": "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
//       "data": {
//         "type": "0x1::coin::CoinStore<0x1::aptos_coin::AptosCoin>",
//         "data": {
//           "coin": {"value": 1000},
//           "frozen": false,
//           "deposit_events": {
//             "counter": 1,
//             "guid": {
//               "id": {
//                 "addr": "0x1234567890abcdef1234567890abcdef",
//                 "creation_num": 2
//               }
//             }
//           },
//           "withdraw_events": {
//             "counter": 1,
//             "guid": {
//               "id": {
//                 "addr": "0x1234567890abcdef1234567890abcdef",
//                 "creation_num": 3
//               }
//             }
//           }
//         }
//       },
//       "type": "write_resource"
//     }],
  
//     "events": [{
//       "key": "0x000000000000000000000000000000000000000000000000000000000000000a",
//       "sequence_number": 1,
//       "type": "0x1::coin::DepositEvent",
//       "data": {
//         "amount": 1000
//       }
//     }]
//   }

pub struct RawTransaction {
    pub type_: String,
    pub version: u64,
    pub hash: String,
    pub state_change_hash: String,
    pub event_root_hash: String,
    pub state_checkpoint_hash: String,
    pub gas_used: u64,
    pub success: bool,
    pub vm_status: String,
    pub accumulator_root_hash: String,
    pub changes: 
    pub timestamp: i64,
    pub epoch: u64,
    pub block_height: u64,
    
}



