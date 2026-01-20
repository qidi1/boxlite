use predicates::prelude::*;

mod common;

#[test]
fn test_run_with_custom_registry() {
    let mut ctx = common::boxlite();
    // This test relies on ghcr.io being available and hosting a 'hello-world' image.
    // A more robust test would involve a local registry, but this is a good start.
    ctx.cmd
        .args(["run", "--rm", "--registry", "ghcr.io", "hello-world:latest"]);
    ctx.cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello from Docker!"));
}

#[test]
fn test_run_with_multiple_registries_fallback() {
    let mut ctx = common::boxlite();
    // First registry is invalid, should fall back to the second one (docker.io).
    ctx.cmd.args([
        "run",
        "--rm",
        "--registry",
        "invalid.registry.that.does.not.exist",
        "--registry",
        "docker.io",
        "alpine:latest",
        "echo",
        "hello from fallback",
    ]);
    ctx.cmd.assert().success().stdout("hello from fallback\n");
}

#[test]
fn test_create_with_custom_registry() {
    let mut ctx = common::boxlite();
    ctx.cmd
        .args(["create", "--registry", "ghcr.io", "hello-world:latest"]);
    let output = ctx.cmd.assert().success().get_output().clone();
    let box_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

    assert!(!box_id.is_empty(), "Box ID should not be empty");

    // Cleanup
    ctx.cleanup_box(&box_id);
}

#[test]
fn test_run_fully_qualified_image_bypasses_registry() {
    let mut ctx = common::boxlite();
    // Provide an invalid registry, but a fully-qualified image name.
    // The pull should succeed because the registry flag is ignored.
    ctx.cmd.args([
        "run",
        "--rm",
        "--registry",
        "invalid.registry.that.does.not.exist",
        "docker.io/library/alpine:latest",
        "echo",
        "fully qualified",
    ]);
    ctx.cmd.assert().success().stdout("fully qualified\n");
}
