use mlua::{UserData, UserDataMethods};

use super::Sds;

impl UserData for Sds {
    fn add_methods<T: UserDataMethods<Self>>(methods: &mut T) {
        methods.add_method("len", |_, this, ()| Ok(this.len()));
        methods.add_method("to_vec", |_, this, ()| Ok(this.to_vec()));
        methods.add_method("as_str", |_, this, ()| {
            Ok(this.as_str().unwrap_or("<invalid utf-8>").to_string())
        });
    }
}
