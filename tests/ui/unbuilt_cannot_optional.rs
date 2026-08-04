// Kit<Unbuilt> cannot call optional() — typestate enforces Ready-only.
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
    let kit = Kit::new();
    // ERROR: optional() is only available on Kit<Ready>
    let _val: Option<Arc<u32>> = kit.optional::<MyModule>();
}
