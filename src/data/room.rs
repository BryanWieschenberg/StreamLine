pub struct Room {
    pub name: String,
    pub whitelist_enabled: bool,
    pub whitelist: Vec<String>,
    pub permissions: HashMap<String, Vec<String>>,
}
