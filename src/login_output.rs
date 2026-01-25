#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoginOutput {
    pub url: Option<String>,
    pub code: Option<String>,
}
