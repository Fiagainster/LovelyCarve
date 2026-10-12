#[path = "build/cli_catalog.rs"]
mod cli_catalog;

fn main() {
    cli_catalog::generate().expect("failed to generate CLI command dispatch");
    // Override tauri-build's default Windows application manifest with one
    // that requests Administrator elevation at launch — but only in release
    // builds. lovelycarve needs admin for raw block-device reads
    // (`\\.\PhysicalDriveN`), live memory capture (BitLocker VMK scan), and
    // Dokan mount control. Shipping the elevated manifest in the installer
    // means a single UAC prompt at first launch and no `ERROR_ACCESS_DENIED`
    // surprises afterwards.
    //
    // Debug builds (`cargo run`, `bun run tauri dev`) keep the default
    // `asInvoker` manifest so the development loop doesn't trigger a UAC
    // prompt on every restart — devs can right-click → Run as administrator
    // when they need to test the privileged code paths.
    //
    // We have to inline the full manifest XML (rather than appending the
    // trustInfo block) because tauri-build's `app_manifest` setter replaces
    // the default outright — the Common-Controls dependency must be
    // preserved here too.
    let mut attributes = tauri_build::Attributes::new();
    #[cfg(target_os = "windows")]
    {
        let is_release = std::env::var("PROFILE").as_deref() == Ok("release");
        if is_release {
            let windows = tauri_build::WindowsAttributes::new().app_manifest(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="requireAdministrator" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
"#,
            );
            attributes = attributes.windows_attributes(windows);
        }
        // Re-run when the active profile flips so the manifest stays in sync.
        println!("cargo:rerun-if-env-changed=PROFILE");
    }
    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}
