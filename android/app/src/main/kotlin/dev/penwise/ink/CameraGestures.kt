package dev.penwise.ink

/**
 * What the two-finger navigation does to the camera: pan and zoom ([ViewportState.transformBy]) and, while the camera
 * is locked to a frame, the dead zone of the overview pinch ([GridPinch]) with its rubber band. UI thread only.
 */
internal class CameraGestures(
    private val viewport: () -> ViewportState,
    private val host: () -> FrameHost?,
) {
    private val pinch = GridPinch()

    fun step(step: NavStep) {
        val camera = viewport()
        val before = pinch.effective
        val open = pinch.step(step.factor)
        // An overview pinch zooms by the pinch's own ratio, so a pinch out followed by a spread ends where it began.
        val factor = if (pinch.armed) pinch.effective / before else step.factor
        camera.transformBy(factor, step.fromX, step.fromY, step.toX, step.toY)
        if (pinch.armed) camera.dipTo(RubberBand.dip(pinch.ratio))
        if (open) host()?.openOverview()
    }

    fun active(active: Boolean) {
        val camera = viewport()
        camera.setGesturing(active)
        if (active) {
            pinch.begin(camera.atFitScale, camera.zoomRatio)
            return
        }
        // Only fingers that lifted can open the overview; a cancel goes through [abort] first.
        if (pinch.end() == GridPinch.Outcome.OPEN_GRID) host()?.openOverview()
        camera.releaseDip()
    }

    /** The gesture was cancelled (pen, palm, a third finger): it opens nothing and the camera returns to fit. */
    fun abort() = pinch.cancel()
}
