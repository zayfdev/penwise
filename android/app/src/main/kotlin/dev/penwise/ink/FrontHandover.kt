package dev.penwise.ink

import android.view.View
import androidx.graphics.surface.SurfaceControlCompat

/**
 * The transactions of the hand-over ([FrontHandoff]). The front layer is hidden with the window frame that first draws
 * the committed stroke. The renderer is not told: it would show the layer again for any render still queued from the
 * stroke (its last samples), with the stroke on it, over the window's copy. Instead the next stroke's first frame,
 * which clears the buffer, shows the layer in its own transaction ([showWithFrame]). Once nothing was drawn for a
 * while the renderer is cleared ([clear]), which also drops the frames it keeps for a commit. UI thread, except
 * [hidden] and [showWithFrame], which the render thread reads.
 */
internal class FrontHandover(
    /** The surface view: its window and its UI thread. */
    private val host: View,
    /** The front-buffered layer, once it has drawn. */
    private val front: () -> SurfaceControlCompat?,
    /** Clears and hides the front buffer through the renderer (`CanvasFrontBufferedRenderer.cancel`). */
    private val clear: () -> Unit,
) {
    private val state = FrontHandoff()
    private var drawnSinceLanded = false
    private val committed =
        object : SurfaceControlCompat.TransactionCommittedListener {
            override fun onTransactionCommitted() {
                host.post(::landed)
            }
        }
    private val trim = Runnable { trimIfIdle() }

    /** The front layer was hidden by a hand-over, behind the renderer's back: the next stroke must show it. */
    @Volatile
    var hidden = false
        private set

    val pending: Boolean get() = state.pending

    fun frontDrawn() {
        state.frontDrawn()
        drawnSinceLanded = true
    }

    fun abandoned() = state.abandoned()

    fun reset() {
        state.reset()
        hidden = false
        host.removeCallbacks(trim)
    }

    /** Hands the committed stroke to [ink], whose next frame draws it. False when the layers are not ready. */
    fun handOver(ink: View): Boolean {
        val layer = front()?.takeIf { it.isValid() } ?: return false
        val root = host.rootSurfaceControl ?: return false
        ink.invalidate()
        state.committed()
        SurfaceControlCompat
            .Transaction()
            .setVisibility(layer, false)
            .addTransactionCommittedListener(host.context.mainExecutor, committed)
            .commitTransactionOnDraw(root)
        return true
    }

    /**
     * Render thread, while a front frame that starts a new stroke completes: true once, when the layer was hidden by a
     * hand-over and must be shown with this frame (the frame cleared the buffer first).
     */
    fun showWithFrame(): Boolean {
        if (!hidden) return false
        hidden = false
        return true
    }

    private fun landed() {
        when (state.landed()) {
            FrontHandoff.Landed.WAIT -> {
                Unit
            }

            FrontHandoff.Landed.CLEAR -> {
                hidden = true
                drawnSinceLanded = false
                host.removeCallbacks(trim)
                host.postDelayed(trim, TRIM_MS)
            }

            FrontHandoff.Landed.SHOW -> {
                val layer = front()?.takeIf { it.isValid() } ?: return
                SurfaceControlCompat.Transaction().setVisibility(layer, true).commit()
            }
        }
    }

    /** Long after the last stroke, with nothing on its way: the renderer forgets it (no render of it is queued now). */
    private fun trimIfIdle() {
        if (state.pending || drawnSinceLanded || !hidden) return
        hidden = false
        clear()
    }

    private companion object {
        /** No render of a committed stroke is still queued this long after its hide landed. */
        const val TRIM_MS = 500L
    }
}
