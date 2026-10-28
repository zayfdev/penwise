package dev.penwise.ink

import android.graphics.Bitmap
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import java.util.concurrent.Executor

/** A picture that failed once comes back later; decoding is capped so a photo never outgrows the cache. */
class PageImagesTest {
    private var now = 1_000L
    private var calls = 0
    private val budgets = ArrayList<Int>()

    private fun images(fetch: (String) -> Bitmap?) =
        PageImages(1_000, { now }, Executor { it.run() }) { asset, maxPixels ->
            calls++
            budgets += maxPixels
            fetch(asset)
        }

    @Test
    fun a_failed_picture_is_not_retried_at_once() {
        val images = images { null }
        assertNull(images.bitmap("a"))
        assertNull(images.bitmap("a"))
        now += PageImages.RETRY_MS - 1
        assertNull(images.bitmap("a"))
        assertEquals(1, calls)
    }

    @Test
    fun a_failed_picture_is_retried_after_the_delay() {
        val images = images { null }
        images.bitmap("a")
        now += PageImages.RETRY_MS
        images.bitmap("a")
        assertEquals(2, calls)
        // The second failure restarts the delay.
        images.bitmap("a")
        assertEquals(2, calls)
    }

    @Test
    fun the_fetch_gets_the_pixel_budget() {
        images { null }.bitmap("a")
        assertEquals(listOf(1_000), budgets)
    }

    @Test
    fun the_sample_size_brings_a_photo_under_the_budget() {
        assertEquals(1, PageImages.sampleSize(1000, 1000, 1_000_000))
        assertEquals(2, PageImages.sampleSize(1001, 1000, 1_000_000))
        assertEquals(4, PageImages.sampleSize(4000, 3000, 1_000_000))
        assertEquals(2, PageImages.sampleSize(8000, 6000, 12_000_000))
        assertEquals(1, PageImages.sampleSize(10, 10, 1_000_000))
    }
}
