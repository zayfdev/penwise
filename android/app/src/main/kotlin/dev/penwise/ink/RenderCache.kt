package dev.penwise.ink

/** 16-byte item id usable as a map key. */
internal class ItemKey(
    val bytes: ByteArray,
) {
    private val hash = bytes.contentHashCode()

    override fun equals(other: Any?): Boolean = other is ItemKey && bytes.contentEquals(other.bytes)

    override fun hashCode(): Int = hash

    companion object {
        const val SIZE = 16

        fun list(blob: ByteArray): List<ItemKey> =
            List(blob.size / SIZE) { ItemKey(blob.copyOfRange(it * SIZE, (it + 1) * SIZE)) }
    }
}

/** What the dry layer must do to show the scene after one [SceneRenderCache.update]. */
internal sealed interface RenderPlan {
    /** Nothing changed. */
    data object Idle : RenderPlan

    /** Repaint the whole page (first frame, reload, resize). */
    data object Full : RenderPlan

    /** New items on top of everything: draw them over the existing layer, no clearing. */
    class Append(
        val items: List<ItemRender>,
    ) : RenderPlan

    /** Clear [area], then redraw the items that touch it, in paint order. */
    class Region(
        val area: Bounds,
    ) : RenderPlan
}

/**
 * Decoded render data of the page, kept in step with the native scene by its dirty list: only items that changed
 * have their floats fetched again, and the plan names the smallest repaint that shows the change. Thread-safe by
 * confinement: call only from the render thread (the scene's own lock covers the native side).
 */
internal class SceneRenderCache(
    private val scene: PageScene,
) {
    private val items = HashMap<ItemKey, ItemRender>()
    private var order: List<ItemKey> = emptyList()

    /** Items in paint order. */
    val paintOrder: List<ItemKey> get() = order

    operator fun get(key: ItemKey): ItemRender? = items[key]

    /** Union of the bounds of [keys] (selection frame); empty when none is drawable. */
    fun boundsOf(keys: Collection<ItemKey>): Bounds =
        Bounds().also { union -> keys.forEach { key -> items[key]?.let { union.add(it.bounds) } } }

    /** Reads what changed since the last call and returns how to repaint it. */
    fun update(): RenderPlan {
        val (dirty, newOrder) =
            scene.atomically { scene.takeDirty() to ItemKey.list(scene.renderOrder(Layers.ALL)) }
        order = newOrder
        if (dirty.isNotEmpty() && dirty[0].toInt() == 1) return reloadAll()
        val delta = Delta()
        val live = HashSet(newOrder)
        val changed = ItemKey.list(dirty.copyOfRange(minOf(1, dirty.size), dirty.size)).toSet()
        for (key in changed + newOrder.filter { it !in items }) reload(key, key in live, delta)
        for (gone in items.keys.filter { it !in live }) reload(gone, false, delta)
        if (delta.area.isEmpty) return RenderPlan.Idle
        val fresh = newOrder.filter { it in delta.fresh && it in items }
        val onTop = !delta.touchedExisting && fresh.isNotEmpty() && newOrder.takeLast(fresh.size) == fresh
        return if (onTop) RenderPlan.Append(fresh.mapNotNull { items[it] }) else RenderPlan.Region(delta.area)
    }

    /** Replaces one item's decoded data (or drops it when it is no longer [live]) and records what that touched. */
    private fun reload(
        key: ItemKey,
        live: Boolean,
        delta: Delta,
    ) {
        val old = items.remove(key)
        old?.let {
            delta.area.add(it.bounds)
            delta.touchedExisting = true
        }
        if (!live) return
        if (old == null) delta.fresh += key
        load(key)?.let {
            items[key] = it
            delta.area.add(it.bounds)
        }
    }

    /** What one [update] changed: the area to repaint, whether any item that was already drawn is involved. */
    private class Delta {
        val area = Bounds()
        var touchedExisting = false
        val fresh = HashSet<ItemKey>()
    }

    /** Drops every decoded item (page switch). The next [update] reports a full repaint. */
    fun reset() {
        items.clear()
        order = emptyList()
    }

    private fun reloadAll(): RenderPlan {
        items.clear()
        for (key in order) load(key)?.let { items[key] = it }
        return RenderPlan.Full
    }

    private fun load(key: ItemKey): ItemRender? {
        val floats = scene.itemFloats(key.bytes)
        return ItemRender.parse(floats) { scene.itemText(key.bytes) }
    }
}
