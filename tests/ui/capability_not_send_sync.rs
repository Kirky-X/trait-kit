// Copyright © 2026 Kirky.X. All rights reserved.
// TC-COMPILE-004: capability not Send+Sync should fail to compile

use trait_kit::prelude::*;

// A trait that is NOT Send+Sync
trait NotSendSync {
    fn do_something(&self);
}

struct BadKey;

impl CapabilityKey for BadKey {
    type Capability = dyn NotSendSync; // WRONG: NotSendSync does not implement Send+Sync
    const NAME: &'static str = "bad";
}

fn main() {}
