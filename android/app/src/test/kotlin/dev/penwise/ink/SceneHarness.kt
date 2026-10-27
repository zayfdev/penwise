package dev.penwise.ink

import dev.penwise.core.Native

/** A real native `SyncCore` with one page and a [PageScene] over it, driven through the host `libpenwise_ffi.so`. */
internal class SceneHarness(
    seed: Int = 1,
    width: Float = 4000f,
    height: Float = 3000f,
) : AutoCloseable {
    val lock = Any()
    val sync: Long = Native.syncNew(ByteArray(16) { (seed + it).toByte() }.also { it[0] = 0 }, seed.toLong())
    private val notebook = Native.syncCreateNotebook(sync, ByteArray(0), "nb", 0, NOW)
    val page: ByteArray =
        Native.syncCreatePage(sync, ByteArray(0), notebook, "p", 0, floatArrayOf(width, height), 0, NOW)

    /** How many commits were announced (the edits themselves reach the core inside the tool's native call). */
    var commits = 0
    val scene =
        PageScene(
            sync,
            lock,
            page,
            entropy = seed.toLong(),
            sink = { commits++ },
            clock = { NOW },
        )

    /** Ids on the page in paint order. */
    fun items(): List<ItemKey> = ItemKey.list(scene.renderOrder())

    fun render(key: ItemKey): ItemRender {
        val floats = scene.itemFloats(key.bytes)
        return requireNotNull(ItemRender.parse(floats) { scene.itemText(key.bytes) })
    }

    /** One undo step: a shape dragged from (x0, y0) to (x1, y1). */
    fun shape(
        mode: ShapeMode,
        box: FloatArray,
        preset: Int = 0,
    ): ItemKey {
        val id = scene.newItemId()
        scene.beginGesture()
        scene.commit(scene.addShape(id, box[0], box[1], box[2], box[3], ShapeStyle(mode, preset, INK, 1f)))
        scene.endGesture()
        return ItemKey(id)
    }

    /** One undo step: a precise (or whole-stroke) eraser chunk along `path` (x, y pairs). */
    fun erase(
        path: FloatArray,
        radius: Float,
        precise: Boolean = true,
    ) {
        scene.beginGesture()
        scene.commit(scene.erase(path, radius, precise))
        scene.endGesture()
    }

    /** Draws a horizontal pen line from [x0] to [x1] at [y] as one undo step; returns its item id. */
    fun line(
        y: Float,
        x0: Float = 20f,
        x1: Float = 620f,
        preset: Int = 0,
    ): ItemKey {
        val count = ((x1 - x0) / STEP).toInt() + 1
        val samples = FloatArray(count * 5)
        val times = IntArray(count)
        for (i in 0 until count) {
            samples[i * 5] = x0 + i * STEP
            samples[i * 5 + 1] = y
            samples[i * 5 + 2] = 0.6f
            times[i] = i * 8_000
        }
        val id = scene.newItemId()
        scene.beginGesture()
        scene.commit(scene.addStroke(id, preset, INK, 1f, 11L, samples, times))
        scene.endGesture()
        return ItemKey(id)
    }

    override fun close() {
        scene.close()
        Native.syncFree(sync)
    }

    companion object {
        const val NOW = 5_000L
        const val INK = 0xFF102030.toInt()
        private const val STEP = 4f
    }
}
