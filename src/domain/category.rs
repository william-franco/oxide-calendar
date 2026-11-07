/// A calendar grouping with its own accent (index into `OxideAccents`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub accent: i32,
    pub visible: bool,
}
