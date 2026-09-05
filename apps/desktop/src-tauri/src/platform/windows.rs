use webcodex_process::SpawnOptions;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn managed_spawn_options() -> SpawnOptions {
    SpawnOptions {
        windows_creation_flags: CREATE_NO_WINDOW,
    }
}
