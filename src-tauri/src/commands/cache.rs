//! 目录/文件级的「按指纹失效」缓存，供大小统计、安全扫描、token 估算复用。
//! 全量重扫每次勾选/部署后都会发生，指纹未变就直接命中，避免重复递归遍历。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

pub(crate) type MtimeCache<V> = Mutex<HashMap<PathBuf, (u64, V)>>;

/// 条目上限，超过直接清空（缓存只为提速，丢了不影响正确性）
const MAX_ENTRIES: usize = 500;

/// 命中返回缓存值；否则用 `compute` 算一次并写入。
/// `fingerprint` 由调用方提供（文件 mtime 或目录内容指纹），同一 key 下变了即失效。
pub(crate) fn cached<V: Clone, F: FnOnce() -> V>(
    cache: &MtimeCache<V>,
    key: PathBuf,
    fingerprint: u64,
    compute: F,
) -> V {
    if let Ok(guard) = cache.lock() {
        if let Some((fp, value)) = guard.get(&key) {
            if *fp == fingerprint {
                return value.clone();
            }
        }
    }
    let value = compute();
    if let Ok(mut guard) = cache.lock() {
        if guard.len() > MAX_ENTRIES {
            guard.clear();
        }
        guard.insert(key, (fingerprint, value.clone()));
    }
    value
}
