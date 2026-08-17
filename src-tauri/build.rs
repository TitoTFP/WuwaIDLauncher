#[cfg(windows)]
use std::{env, fs, path::PathBuf};

fn main() {
    let attributes = tauri_build::Attributes::new().windows_attributes(
        tauri_build::WindowsAttributes::new_without_app_manifest(),
    );
    tauri_build::try_build(attributes).expect("failed to run tauri build script");

    #[cfg(windows)]
    {
        let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
        let manifest_path = out_dir.join("wuwaid-app.manifest");
        fs::write(
            &manifest_path,
            r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
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
</assembly>
"#,
        )
        .expect("write Windows application manifest");

        println!("cargo:rerun-if-changed=build.rs");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
            manifest_path.display()
        );
    }
}
