use crate::namespace::StorageNamespace;

struct TestResource;
impl StorageNamespace for TestResource {
    const PREFIX: [u8; 4] = [0x52, 0x45, 0x53, 0x4f];
}

struct TestTariff;
impl StorageNamespace for TestTariff {
    const PREFIX: [u8; 4] = [0x54, 0x41, 0x52, 0x49];
}

struct TestSettlement;
impl StorageNamespace for TestSettlement {
    const PREFIX: [u8; 4] = [0x53, 0x45, 0x54, 0x4c];
}

struct TestCommon;
impl StorageNamespace for TestCommon {
    const PREFIX: [u8; 4] = [0x43, 0x4f, 0x4d, 0x4d];
}

#[test]
fn test_namespace_prefixes_are_unique() {
    let prefixes = [
        TestResource::PREFIX,
        TestTariff::PREFIX,
        TestSettlement::PREFIX,
        TestCommon::PREFIX,
    ];
    for i in 0..prefixes.len() {
        for j in (i + 1)..prefixes.len() {
            assert_ne!(
                prefixes[i], prefixes[j],
                "Namespace prefix collision between {} and {}",
                i, j
            );
        }
    }
}

#[test]
fn test_scoped_key_prepends_prefix() {
    let raw = b"test_data";
    let scoped = TestResource.scoped_key(raw);
    assert_eq!(&scoped[..4], &[0x52, 0x45, 0x53, 0x4f]);
    assert_eq!(&scoped[4..], raw);

    let scoped2 = TestTariff.scoped_key(raw);
    assert_eq!(&scoped2[..4], &[0x54, 0x41, 0x52, 0x49]);
    assert_eq!(&scoped2[4..], raw);
}

#[test]
fn test_scoped_key_length() {
    let raw = b"0123456789abcdef";
    let scoped = TestCommon.scoped_key(raw);
    assert_eq!(scoped.len(), 4 + raw.len());
}
