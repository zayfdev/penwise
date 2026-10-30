//! Viewport control for one inspected [`Page`]: the frame host changes it when the preset changes, and the
//! bridge tests use it to exercise the layout's resize path.

use super::{Error, Page};

impl Page {
    /// Changes the page's viewport, as a frame host does when the preset changes. Used by the bridge tests to
    /// exercise the layout's resize path.
    ///
    /// # Errors
    /// A viewport outside `1..=8000` on either side, or a failed CDP command.
    #[cfg(test)]
    pub(crate) async fn resize(&self, width: u32, height: u32) -> Result<(), Error> {
        if width == 0 || height == 0 || width > super::MAX_VIEWPORT || height > super::MAX_VIEWPORT
        {
            return Err(Error::Invalid(format!(
                "viewport must be 1..={} on both sides",
                super::MAX_VIEWPORT
            )));
        }
        self.set_viewport(width, height).await
    }

    pub(super) async fn setup(&self, width: u32, height: u32) -> Result<(), Error> {
        for domain in ["Page.enable", "DOM.enable", "CSS.enable"] {
            self.command(domain, serde_json::json!({})).await?;
        }
        self.set_viewport(width, height).await
    }

    pub(super) async fn set_viewport(&self, width: u32, height: u32) -> Result<(), Error> {
        self.command(
            "Emulation.setDeviceMetricsOverride",
            serde_json::json!({
                "width": width,
                "height": height,
                "deviceScaleFactor": 1,
                "mobile": false,
                "screenWidth": width,
                "screenHeight": height,
            }),
        )
        .await?;
        Ok(())
    }
}
