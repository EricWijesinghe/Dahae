// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

fn main() {
    // sqlx::migrate! embeds files at compile time; rebuild when a migration is added.
    println!("cargo:rerun-if-changed=../../migrations");
}
