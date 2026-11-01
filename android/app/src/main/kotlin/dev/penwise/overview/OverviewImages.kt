package dev.penwise.overview

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.util.LruCache
import dev.penwise.frames.FrameHttp
import dev.penwise.frames.FrameServer
import dev.penwise.frames.StillClient
import dev.penwise.frames.StillReply
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.withContext

/** Which power-of-two shrink a picture is decoded at. Pure, so it is unit-tested. */
internal object ImageSampling {
    /**
     * The largest `inSampleSize` that keeps a [width] x [height] picture at least [targetWidth] wide, raised until its
     * longer side fits [maxSide] (a texture the GPU can draw) and it has at most [maxPixels] pixels (memory).
     */
    fun sample(
        width: Int,
        height: Int,
        targetWidth: Int,
        maxSide: Int,
        maxPixels: Long = Long.MAX_VALUE,
    ): Int {
        var sample = 1
        while (width / (sample * 2) >= targetWidth.coerceAtLeast(1)) sample *= 2
        while (maxOf(width, height) / sample > maxSide) sample *= 2
        while (width.toLong() * height / (sample.toLong() * sample) > maxPixels) sample *= 2
        return sample
    }
}

/**
 * The pictures the overview and the viewer show, as bitmaps: a frame's still (its first strip, shrunk by the server to
 * [CARD_WIDTH] with `?w=`), and a kept picture's asset (`GET /assets/<id>`). Cards live in one bounded cache and the
 * viewer's big pictures in another, so opening a tall picture never evicts the cards. The instance outlives the
 * overview, so reopening it finds its pictures at once ([peek]). Every fetch blocks, so it runs on the IO dispatcher.
 */
internal class OverviewImages(
    private val server: () -> FrameServer?,
) {
    private val cards = bitmapCache(CARD_CACHE_BYTES)
    private val viewer = bitmapCache(VIEWER_CACHE_BYTES)

    /** Frame id to the cache key of the still that was loaded for it last. */
    private val stillKeys = java.util.concurrent.ConcurrentHashMap<String, String>()

    private fun bitmapCache(bytes: Int) =
        object : LruCache<String, Bitmap>(bytes) {
            override fun sizeOf(
                key: String,
                value: Bitmap,
            ): Int = value.allocationByteCount
        }

    /** A card's bitmap if it is in memory, else null. Never fetches. */
    fun peek(image: CardImage): Bitmap? =
        when (image) {
            is CardImage.Still -> stillKeys[image.frameId]?.let { cards.get(it) }
            is CardImage.Asset -> cards.get(assetKey(image.asset, CARD_WIDTH))
            is CardImage.Placeholder -> null
        }

    /** Fetches run here, so a hung server never holds up the caller beyond its time limit. */
    private val background = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    /** The current bitmap fetched within [timeoutMs], else the one in memory; null when neither (it opens anyway). */
    suspend fun preload(
        image: CardImage,
        timeoutMs: Long,
    ): Bitmap? = CardLoading.within(background, timeoutMs, { peek(image) }) { card(image, PRELOAD_TRIES) }

    /** The bitmap of a card, or null while the server is away (or for a placeholder). */
    suspend fun card(
        image: CardImage,
        tries: Int = STILL_TRIES,
    ): Bitmap? =
        when (image) {
            is CardImage.Still -> still(image.frameId, tries)
            is CardImage.Asset -> asset(image.asset, CARD_WIDTH, cards, MAX_SIDE, Long.MAX_VALUE)
            is CardImage.Placeholder -> null
        }

    /** A kept picture for the viewer, at least [widthPx] wide when the picture is, within the viewer's pixel budget. */
    suspend fun picture(
        asset: String,
        widthPx: Int,
    ): Bitmap? = asset(asset, widthPx, viewer, VIEWER_MAX_SIDE, VIEWER_MAX_PIXELS)

    private suspend fun still(
        frameId: String,
        tries: Int,
    ): Bitmap? =
        CardLoading.untilReady(tries) {
            withContext(Dispatchers.IO) { stillOnce(frameId) }
        }

    private fun stillOnce(frameId: String): CardLoading.Attempt<Bitmap> {
        val server = server() ?: return CardLoading.Attempt.Failed
        val client = StillClient(server)
        val manifest =
            when (val reply = client.manifest(frameId, null)) {
                is StillReply.Busy -> return CardLoading.Attempt.Busy(reply.retryAfterMs)
                is StillReply.Fresh -> reply.manifest
                else -> null
            }
        val strip = manifest?.strips?.firstOrNull() ?: return CardLoading.Attempt.Failed
        val url = strip.url + (if ('?' in strip.url) "&" else "?") + "w=$CARD_WIDTH"
        stillKeys[frameId] = url
        val bitmap = cached(cards, url, Decode(CARD_WIDTH, MAX_SIDE, Long.MAX_VALUE)) { client.strip(url) }
        return bitmap?.let { CardLoading.Attempt.Ready(it) } ?: CardLoading.Attempt.Failed
    }

    private suspend fun asset(
        asset: String,
        widthPx: Int,
        cache: LruCache<String, Bitmap>,
        maxSide: Int,
        maxPixels: Long,
    ): Bitmap? =
        withContext(Dispatchers.IO) {
            val server = server() ?: return@withContext null
            cached(cache, assetKey(asset, widthPx), Decode(widthPx, maxSide, maxPixels)) {
                FrameHttp(server).get("/assets/$asset")?.takeIf { it.ok }?.body
            }
        }

    private fun assetKey(
        asset: String,
        widthPx: Int,
    ) = "asset:$asset:$widthPx"

    /** What a decode may use: the width it wants and the limits on its sides and pixels. */
    private class Decode(
        val width: Int,
        val maxSide: Int,
        val maxPixels: Long,
    )

    private fun cached(
        cache: LruCache<String, Bitmap>,
        key: String,
        decode: Decode,
        bytes: () -> ByteArray?,
    ): Bitmap? {
        cache.get(key)?.let { return it }
        val data = bytes() ?: return null
        return decode(data, decode)?.also { cache.put(key, it) }
    }

    private fun decode(
        bytes: ByteArray,
        decode: Decode,
    ): Bitmap? {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        if (bounds.outWidth <= 0) return null
        val options =
            BitmapFactory.Options().apply {
                inSampleSize =
                    ImageSampling.sample(
                        bounds.outWidth,
                        bounds.outHeight,
                        decode.width,
                        decode.maxSide,
                        decode.maxPixels,
                    )
            }
        return BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)
    }

    private companion object {
        /** The strip width the server shrinks a card's still to. */
        const val CARD_WIDTH = 360

        /** A still the server is capturing is asked for again after its `Retry-After`, this many times at most. */
        const val STILL_TRIES = 6

        /** The preload runs detached from the overview, so it asks once; the cards' own loads do the retrying. */
        const val PRELOAD_TRIES = 1
        const val MAX_SIDE = 4096
        const val CARD_CACHE_BYTES = 48 * 1024 * 1024

        /** The viewer keeps its own (a 1440 x 7574 picture is 43 MB), and never decodes more than 16 M pixels. */
        const val VIEWER_CACHE_BYTES = 96 * 1024 * 1024
        const val VIEWER_MAX_SIDE = 8192
        const val VIEWER_MAX_PIXELS = 16_000_000L
    }
}
