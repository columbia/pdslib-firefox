use std::hash::{DefaultHasher, Hash as _, Hasher as _};

pub fn ad_hash(ad: impl AsRef<str>) -> u32 {
    let mut hasher = DefaultHasher::new();
    ad.as_ref().hash(&mut hasher);
    (hasher.finish() >> 32) as u32
}

pub fn filter_data(ad: impl AsRef<str>, index: impl Into<u64>) -> u64 {
    // filter_data is u64:
    // lower 32 bits are the histogram index
    // upper 32 bits are the ad/campaign id, hashed

    let ad_hash = ad_hash(ad) as u64;
    let index = index.into();
    (ad_hash << 32) | index
}
