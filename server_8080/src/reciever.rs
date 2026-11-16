/// checks if a username is valid
// you normally shouldn't  return string literals as an Err, but this is only used in one place anyway so it is fine
pub fn username_check(username: &str) -> Result<(), &'static str> {
    match username.trim() {
        "" => Err("invalid username: an empty username isn't allowed"),
        "me" => Err("invalid username: me"),
        "server" => Err("invalid username: server"),
        with_space if with_space.contains(' ') => {
            Err("invalid username: a username isn't allowed to have spaces")
        }
        _ => Ok(()),
    }
}
