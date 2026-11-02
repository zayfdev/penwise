package dev.penwise.ink

/**
 * What the dry layer must know to take a background render in: the view and style it was started for, and how much
 * of the scene changed since its snapshot. A render for the right view is accepted even when the scene moved on in
 * the meantime (a remote client drawing, an erase sweep): the changed area is repainted onto it, so the render never
 * has to be restarted and cannot starve. Only a change that cannot be located (picture arrival, paper style, resize)
 * voids it. Pure state, no Android types.
 */
internal class SettleLedger {
    private var view: ViewTransform? = null
    private var style: PageStyle? = null
    private val drift = Bounds()
    private var driftAll = false

    /** The scene changed by [plan] (or in an unlocatable way when [everything]); recorded while a render is open. */
    fun sceneChanged(
        plan: RenderPlan,
        everything: Boolean,
    ) {
        if (view == null) return
        when {
            everything || plan is RenderPlan.Full -> driftAll = true
            plan is RenderPlan.Append -> plan.items.forEach { drift.add(it.bounds) }
            plan is RenderPlan.Region -> drift.add(plan.area)
        }
    }

    /**
     * Is a repairable render open that the layer can show at [view] / [style] once it lands (the same zoom and style;
     * an earlier pan is fine)? Then a new one for [view] would only cut it short.
     */
    fun runs(
        view: ViewTransform,
        style: PageStyle,
    ): Boolean = this.view?.scale == view.scale && this.style == style && !driftAll

    /** A render for [view] / [style] was requested from the scene as it is now. */
    fun submitted(
        view: ViewTransform,
        style: PageStyle,
    ) {
        this.view = view
        this.style = style
        drift.clear()
        driftAll = false
    }

    /** Nothing is open any more (cancelled, or the bitmap was painted in place). */
    fun clear() {
        view = null
        style = null
        drift.clear()
        driftAll = false
    }

    /**
     * A render of [rendered] finished while the camera is at [view] with [style]. Null: unusable, drop it. Otherwise
     * the page-space area the scene changed since its snapshot, to repaint onto it (empty when it is current).
     *
     * A render for an earlier pan at the same zoom is usable: a finger scroll moves the camera every frame, so a
     * render always finishes behind it, and it is shown through the transform like the bitmap it replaces. Without
     * that no render landed during a scroll and strokes that came into view stayed cut at the old screen edge. A
     * render for another zoom is dropped (shown scaled it would be blurred).
     */
    fun accept(
        rendered: ViewTransform,
        view: ViewTransform,
        style: PageStyle,
    ): Bounds? {
        val usable = rendered.scale == view.scale && this.view == rendered && this.style == style && !driftAll
        val area = drift.copy()
        clear()
        return area.takeIf { usable }
    }
}
