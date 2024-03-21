use crate::{utilities::right_pad, Error, Precompile, PrecompileResult, PrecompileWithAddress};
use revm_primitives::{alloy_primitives::B512, Bytes, B256};

pub const ECRECOVER: PrecompileWithAddress = PrecompileWithAddress(
    crate::u64_to_address(1),
    Precompile::Standard(ec_recover_run),
);

pub use self::secp256k1::ecrecover;

#[cfg(not(feature = "secp256k1"))]
#[allow(clippy::module_inception)]
mod secp256k1 {
    use k256::ecdsa::{Error, RecoveryId, Signature, VerifyingKey};
    use revm_primitives::{alloy_primitives::B512, keccak256, B256};

    pub fn ecrecover(sig: &B512, mut recid: u8, msg: &B256) -> Result<B256, Error> {
        // parse signature
        let mut sig = Signature::from_slice(sig.as_slice())?;

        // normalize signature and flip recovery id if needed.
        if let Some(sig_normalized) = sig.normalize_s() {
            sig = sig_normalized;
            recid ^= 1;
        }
        let recid = RecoveryId::from_byte(recid).expect("recovery ID is valid");

        // recover key
        let recovered_key = VerifyingKey::recover_from_prehash(&msg[..], &sig, recid)?;
        // hash it
        let mut hash = keccak256(
            &recovered_key
                .to_encoded_point(/* compress = */ false)
                .as_bytes()[1..],
        );

        // truncate to 20 bytes
        hash[..12].fill(0);
        Ok(hash)
    }
}

#[cfg(feature = "secp256k1")]
#[allow(clippy::module_inception)]
mod secp256k1 {
    // Silence the unused crate dependency warning.
    use k256 as _;
    use revm_primitives::{alloy_primitives::B512, keccak256, B256};
    use secp256k1::{
        ecdsa::{RecoverableSignature, RecoveryId},
        Message,
        Secp256k1,
    };

    pub fn ecrecover(sig: &B512, recid: u8, msg: &B256) -> Result<B256, secp256k1::Error> {
        let recid = RecoveryId::from_i32(recid as i32).expect("recovery ID is valid");
        let sig = RecoverableSignature::from_compact(sig.as_slice(), recid)?;

        let secp = Secp256k1::new();
        let msg = Message::from_digest(msg.0);
        let public = secp.recover_ecdsa(&msg, &sig)?;

        let mut hash = keccak256(&public.serialize_uncompressed()[1..]);
        hash[..12].fill(0);
        Ok(hash)
    }
}

#[cfg(feature = "std")]
fn ec_recover_run(input: &Bytes, gas_limit: u64) -> PrecompileResult {
    const ECRECOVER_BASE: u64 = 3_000;
    if ECRECOVER_BASE > gas_limit {
        return Err(Error::OutOfGas);
    }
    let input = right_pad::<128>(input);
    // `v` must be a 32-byte big-endian integer equal to 27 or 28.
    if !(input[32..63].iter().all(|&b| b == 0) && matches!(input[63], 27 | 28)) {
        return Ok((ECRECOVER_BASE, Bytes::new()));
    }
    let msg = <&B256>::try_from(&input[0..32]).unwrap();
    let recid = input[63] - 27;
    let sig = <&B512>::try_from(&input[64..128]).unwrap();
    let out = secp256k1::ecrecover(sig, recid, msg)
        .map(|o| o.to_vec().into())
        .unwrap_or_default();
    Ok((ECRECOVER_BASE, out))
}

#[cfg(not(feature = "std"))]
#[link(wasm_import_module = "fluentbase_v1alpha")]
extern "C" {
    fn _crypto_keccak256(data_offset: *const u8, data_len: u32, output32_offset: *mut u8);
    fn _crypto_ecrecover(
        digest32_offset: *const u8,
        sig64_offset: *const u8,
        output65_offset: *mut u8,
        rec_id: u32,
    );
}

#[cfg(not(feature = "std"))]
fn ec_recover_run(input: &Bytes, gas_limit: u64) -> PrecompileResult {
    const ECRECOVER_BASE: u64 = 3_000;
    if ECRECOVER_BASE > gas_limit {
        return Err(Error::OutOfGas);
    }
    let input = right_pad::<128>(input);
    // `v` must be a 32-byte big-endian integer equal to 27 or 28.
    if !(input[32..63].iter().all(|&b| b == 0) && matches!(input[63], 27 | 28)) {
        return Ok((ECRECOVER_BASE, Bytes::new()));
    }
    let mut public_key: [u8; 65] = [0u8; 65];
    let mut hash: B256 = B256::ZERO;
    unsafe {
        (
            input[0..32].as_ptr(),
            input[64..128].as_ptr(),
            public_key.as_mut_ptr(),
            input[63] - 27,
        );
        LowLevelSDK::crypto_keccak256(public_key[1..].as_ptr(), 64, hash.as_mut_ptr())
    }
    hash[..12].fill(0);
    Ok((ECRECOVER_BASE, Bytes::from(hash)))
}

// #[cfg(test)]
// mod tests {
//     use crate::{
//         secp256k1::{ec_recover_run, ec_recover_run_std},
//         Bytes,
//     };
//     use revm_primitives::{hex, keccak256};
//
//     #[test]
//     fn test_ecrecover() {
//         let digest32 = keccak256("IT People test of ecrecover.");
//         let sig64 =
// hex!("c46cdc50a66f4d07c6e9a127a7277e882fb21bcfb5b068f2b58c7f7283993b790bdb5f0ac79d1a7efdc255f399a045038c1b433e9d06c1b1abd58a5fcaab33f1"
// );
//
//         let mut input: Vec<u8> = vec![];
//         input.extend(&digest32);
//         (0..31).for_each(|_| input.push(0));
//         input.push(0x1c);
//         input.extend(&sig64);
//         let input = Bytes::from(input);
//         let (gas, result) = ec_recover_run_std(&input, 1_000_000).unwrap();
//         println!("gas used: {}", gas);
//         let (_, result2) = ec_recover_run(&input, 1_000_000).unwrap();
//         assert_eq!(result, result2)
//     }
// }
