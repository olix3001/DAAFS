use nbdkit::Server;

/// Basic struct representing this plugin.
struct DiscordDrivePlugin {}

/// Default implementation of the plugin.
impl Default for DiscordDrivePlugin {
    fn default() -> Self {
        Self {}
    }
}

/// Implementation of the plugin.
impl Server for DiscordDrivePlugin {
    fn get_size(&self) -> nbdkit::Result<i64> {
        Ok(
            env!("DEVICE_SIZE")
                .parse()
                .expect("Failed to parse DEVICE_SIZE from env")
        )
    }

    fn name() -> &'static str where Self: Sized {
        "discorddrive"
    }

    fn open(_readonly: bool) -> nbdkit::Result<Box<dyn Server>> where Self: Sized {
        Ok(Box::new(Self::default()))
    }

    fn read_at(&self, buf: &mut [u8], offset: u64) -> nbdkit::Result<()> {
        todo!()
    }
}

// Entry point for the plugin.
nbdkit::plugin!(DiscordDrivePlugin {});