package dev.penwise.frames

/**
 * Which frame the camera is locked to (locked-frames contract §3.1). On a board page with frames the lock is always
 * set; the locked frame is the live one (`LivePages` keeps the frame live before it warm). Plain state, no camera
 * and no views: [LayerLock] moves the camera. UI thread only.
 */
class FrameLock {
    /** The locked frame's id, or null while the page has no frames. */
    var locked: String? = null
        private set

    /** The lane of the frame locked last, so a frame that is deleted hands the lock to its lane's live frame. */
    private var lane: LaneKey? = null
    private var wanted: String? = null

    /** Locks [spec]; true when that changed the locked frame. */
    internal fun lockTo(spec: FrameSpec): Boolean {
        wanted = null
        lane = spec.lane
        if (spec.id == locked) return false
        locked = spec.id
        return true
    }

    /** [id] is wanted but its frame may not be known yet (an agent's `Focus` that arrived with a page switch). */
    fun want(id: String) {
        wanted = id
    }

    /**
     * Brings the lock in line with the frames now: a wanted frame that exists wins, a locked frame that is gone is
     * replaced by its lane's live frame (or [FrameOpening]'s pick), and a page without frames has no lock. True when
     * the locked frame is now another one.
     */
    internal fun reconcile(specs: List<FrameSpec>): Boolean {
        if (specs.isEmpty()) {
            clear()
            return false
        }
        val request = wanted?.let { id -> specs.firstOrNull { it.id == id } }
        if (request != null) return lockTo(request)
        val current = specs.firstOrNull { it.id == locked }
        if (current != null) {
            lane = current.lane
            return false
        }
        val next = lane?.let { FrameLane.live(specs, it)?.id } ?: FrameOpening.target(specs)
        return specs.firstOrNull { it.id == next }?.let(::lockTo) ?: false
    }

    /** Forgets everything (the page was left). */
    fun clear() {
        locked = null
        lane = null
        wanted = null
    }
}
