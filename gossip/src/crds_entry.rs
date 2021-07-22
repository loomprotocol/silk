use {
    crate::{
        contact_info::ContactInfo,
        crds::VersionedCrdsValue,
        crds_value::{
            CrdsData, CrdsValue, CrdsValueLabel, LegacyVersion, LowestSlot, SnapshotHash, Version,
        },
    },
    indexmap::IndexMap,
    solana_sdk::pubkey::Pubkey,
};

type CrdsTable = IndexMap<CrdsValueLabel, VersionedCrdsValue>;

/// Represents types which can be looked up from crds table given a key. e.g.
///   CrdsValueLabel -> VersionedCrdsValue, CrdsValue, CrdsData
///   Pubkey -> ContactInfo, LowestSlot, SnapshotHash, ...
pub trait CrdsEntry<'a, 'b>: Sized {
    type Key; // Lookup key.
    fn get_entry(table: &'a CrdsTable, key: Self::Key) -> Option<Self>;
}

macro_rules! impl_crds_entry (
    // Lookup by CrdsValueLabel.
    ($name:ident, |$entry:ident| $body:expr) => (
        impl<'a, 'b> CrdsEntry<'a, 'b> for &'a $name {
            type Key = &'b CrdsValueLabel;
            fn get_entry(table:&'a CrdsTable, key: Self::Key) -> Option<Self> {
                let $entry = table.get(key);
                $body
            }
        }
    );
    // Lookup by Pubkey.
    ($name:ident, $pat:pat, $expr:expr) => (
        impl<'a, 'b> CrdsEntry<'a, 'b> for &'a $name {
            type Key = Pubkey;
            fn get_entry(table:&'a CrdsTable, key: Self::Key) -> Option<Self> {
                let key = CrdsValueLabel::$name(key);
                match &table.get(&key)?.value.data {
                    $pat => Some($expr),
                    _ => None,
                }
            }
        }
    );
);

// Lookup by CrdsValueLabel.
impl_crds_entry!(CrdsData, |entry| Some(&entry?.value.data));
impl_crds_entry!(CrdsValue, |entry| Some(&entry?.value));
impl_crds_entry!(VersionedCrdsValue, |entry| entry);

// Lookup by Pubkey.
impl_crds_entry!(ContactInfo, CrdsData::ContactInfo(node), node);
impl_crds_entry!(LegacyVersion, CrdsData::LegacyVersion(version), version);
impl_crds_entry!(LowestSlot, CrdsData::LowestSlot(_, slot), slot);
impl_crds_entry!(Version, CrdsData::Version(version), version);

impl<'a, 'b> CrdsEntry<'a, 'b> for &'a SnapshotHash {
    type Key = Pubkey;
    fn get_entry(table: &'a CrdsTable, key: Self::Key) -> Option<Self> {
        let key = CrdsValueLabel::SnapshotHashes(key);
        match &table.get(&key)?.value.data {
            CrdsData::SnapshotHashes(snapshot_hash) => Some(snapshot_hash),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{crds::Crds, crds_value::new_rand_timestamp},
        rand::seq::SliceRandom,
        solana_sdk::signature::Keypair,
        std::collections::HashMap,
    };

    #[test]
    fn test_get_crds_entry() {
        let mut rng = rand::thread_rng();
        let mut crds = Crds::default();
        let keypairs: Vec<_> = std::iter::repeat_with(Keypair::new).take(32).collect();
        let mut entries = HashMap::new();
        for _ in 0..256 {
            let keypair = keypairs.choose(&mut rng).unwrap();
            let value = CrdsValue::new_rand(&mut rng, Some(keypair));
            let key = value.label();
            if let Ok(()) = crds.insert(value.clone(), new_rand_timestamp(&mut rng)) {
                entries.insert(key, value);
            }
        }
        assert!(crds.len() > 64);
        assert_eq!(crds.len(), entries.len());
        for entry in entries.values() {
            let key = entry.label();
            assert_eq!(crds.get::<&CrdsValue>(&key), Some(entry));
            assert_eq!(crds.get::<&CrdsData>(&key), Some(&entry.data));
            assert_eq!(crds.get::<&VersionedCrdsValue>(&key).unwrap().value, *entry);
            let key = entry.pubkey();
            match &entry.data {
                CrdsData::ContactInfo(node) => {
                    assert_eq!(crds.get::<&ContactInfo>(key), Some(node))
                }
                CrdsData::LowestSlot(_, slot) => {
                    assert_eq!(crds.get::<&LowestSlot>(key), Some(slot))
                }
                CrdsData::Version(version) => assert_eq!(crds.get::<&Version>(key), Some(version)),
                CrdsData::LegacyVersion(version) => {
                    assert_eq!(crds.get::<&LegacyVersion>(key), Some(version))
                }
                CrdsData::SnapshotHashes(hash) => {
                    assert_eq!(crds.get::<&SnapshotHash>(key), Some(hash))
                }
                _ => (),
            }
        }
    }
}
