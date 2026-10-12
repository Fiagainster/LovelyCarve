use super::*;

#[test]
fn symlinks_and_junctions_preserve_evidence_targets() {
    for (tag, header) in [(0xa000_000cu32, 12), (0xa000_0003, 8)] {
        let target = r"\??\C:\Users\测试";
        let names: Vec<_> = target.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut data = vec![0; header];
        data[2..4].copy_from_slice(&(names.len() as u16).to_le_bytes());
        data.extend(names);
        let mut raw = tag.to_le_bytes().to_vec();
        raw.extend((data.len() as u16).to_le_bytes());
        raw.extend([0, 0]);
        raw.extend(data);
        assert_eq!(
            ReparsePoint::parse(&raw).unwrap().link_target().unwrap(),
            Some(target.into())
        );
        raw[10..12].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(ReparsePoint::parse(&raw).unwrap().link_target().is_err());
    }
    assert!(ReparsePoint::parse(&[0; 7]).is_err());
    assert!(ReparsePoint::parse(&[0; 9]).is_err());
}
