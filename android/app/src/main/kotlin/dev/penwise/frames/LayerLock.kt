package dev.penwise.frames

import dev.penwise.ink.PageRect
import dev.penwise.ink.ViewportState
import dev.penwise.ink.lockTo
import dev.penwise.ink.relock
import dev.penwise.ink.unlock

/**
 * The lock between the frames and the camera (locked-frames contract §3.1): [FrameLock] says which frame, this moves
 * the camera to its box and makes the frame live. Locking a frame the camera cannot reach yet (no screen size, or
 * the frame not known yet) waits: [settle] and the next [framesChanged] finish it. UI thread only.
 */
internal class LayerLock(
    private val camera: () -> ViewportState?,
    private val specs: () -> List<FrameSpec>,
    /** Makes a frame the live page (null: none). */
    private val focus: (String?) -> Unit,
    /** The locked frame changed (null: no lock). */
    private val changed: (String?) -> Unit,
) {
    val state = FrameLock()

    /** The locked frame has not been flown to yet. */
    private var pending = false
    private var box: PageRect? = null
    private var announced: String? = null

    /** The camera last locked, so releasing still frees it after the layer let go of its viewport. */
    private var used: ViewportState? = null

    /** The locked frame's id. */
    val locked: String? get() = state.locked

    /** Locks [id]: the camera slides to its box in [slideMs] and the frame goes live. */
    fun lockTo(
        id: String,
        slideMs: Long = SLIDE_MS,
    ) {
        val spec = specs().firstOrNull { it.id == id }
        if (spec == null) {
            state.want(id)
            return
        }
        state.lockTo(spec)
        pending = false
        box = spec.box
        camera()?.let {
            used = it
            it.lockTo(spec.box, slideMs)
        }
        focus(id)
        announce(id)
    }

    /** The frames changed: keeps the lock valid and the camera on the locked frame's box. */
    fun framesChanged() {
        val frames = specs()
        val moved = state.reconcile(frames)
        val id = state.locked
        if (id == null) {
            release()
            return
        }
        val spec = frames.first { it.id == id }
        val before = box
        when {
            moved || pending -> {
                arrive(id)
            }

            before != null && (before.x != spec.x || before.y != spec.y) -> {
                lockTo(id)
            }

            before != spec.box -> {
                box = spec.box
                camera()?.relock(spec.box)
            }
        }
    }

    /**
     * The camera has settled: a lock that waited for the screen size is flown to now. True while it still waits (the
     * caller settles again later).
     */
    fun settle(): Boolean {
        if (!pending) return false
        arrive(state.locked ?: return false)
        return pending
    }

    /** The page was left or has no frames: the camera is free. */
    fun release() {
        pending = false
        box = null
        state.clear()
        (camera() ?: used)?.unlock()
        used = null
        announce(null)
    }

    private fun announce(id: String?) {
        if (id == announced) return
        announced = id
        changed(id)
    }

    private fun arrive(id: String) {
        if (camera()?.hasSize == true) {
            lockTo(id, OPEN_MS)
        } else {
            pending = true
        }
    }

    private companion object {
        /** A switch between frames: a fast slide (contract §3.2: at most 180 ms). */
        const val SLIDE_MS = 160L

        /** The first fit of a board that just got its frames. */
        const val OPEN_MS = 220L
    }
}
