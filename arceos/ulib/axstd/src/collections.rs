pub use alloc::collections::*;
mod hash_map; // 声明子模块
pub use self::hash_map::HashMap; // 把类型公开到 collections 下
