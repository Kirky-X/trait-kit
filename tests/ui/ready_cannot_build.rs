// Kit<Ready> cannot call build() — typestate enforces Unbuilt-only.
use std::sync::Arc;
use trait_kit::prelude::*;

struct MyModule;
impl_module_meta!(MyModule, "my-module");
impl AutoBuilder for MyModule {
    type Capability = Arc<u32>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(1))
    }
}

fn main() {
    let mut kit = Kit::new();
    kit.register::<MyModule>().unwrap();
    let ready = kit.build().unwrap();
    // ERROR: build() is only available on Kit<Unbuilt>
    let _double = ready.build().unwrap();
}
