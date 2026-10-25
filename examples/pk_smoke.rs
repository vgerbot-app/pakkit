//! Smoke test for the PackageKit D-Bus path: probe, refresh cache (no auth
//! needed), and resolve-then-fail on a bogus package (no auth prompt, since
//! RemovePackages is never reached).

// Examples link the package's deps but only name a few of them.
#![allow(unused_crate_dependencies)]

#[cfg(target_os = "linux")]
fn main() {
    use pakkit::backends::packagekit::{self, PkOp};

    println!("available: {}", packagekit::available());

    let out = packagekit::run(&PkOp::RefreshCache);
    match &out {
        Ok(o) => println!("refresh: status={} stderr={:?}", o.status, o.stderr),
        Err(e) => println!("refresh error: {e}"),
    }

    let bogus = PkOp::Remove {
        name: "definitely-not-a-real-package-xyz".to_owned(),
    };
    match packagekit::run(&bogus) {
        Ok(o) => println!("bogus remove unexpectedly ok: {o:?}"),
        Err(packagekit::RunError::Transaction(o)) => {
            println!("bogus remove: status={} stderr={:?}", o.status, o.stderr)
        }
        Err(e) => println!("bogus remove transport error: {e}"),
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {}
