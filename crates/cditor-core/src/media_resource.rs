#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MediaResourceId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MediaStableBox {
    pub estimated_height: f64,
    pub min_height: f64,
    pub max_height: f64,
}
