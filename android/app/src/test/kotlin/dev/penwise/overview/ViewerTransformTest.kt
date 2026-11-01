package dev.penwise.overview

import dev.penwise.ink.PageRect
import org.junit.Assert.assertEquals
import org.junit.Test

/** The viewer's zoom clamp, the picture sampling and where a new screen goes. */
class ViewerTransformTest {
    private val size = ViewerSize.fitted(1440, 7574, 1000f, 600f)

    @Test
    fun a_picture_fits_the_screens_width_and_keeps_its_ratio() {
        assertEquals(1000f, size.pictureWidth, 0.01f)
        assertEquals(1000f * 7574 / 1440, size.pictureHeight, 0.01f)
    }

    @Test
    fun it_never_shrinks_below_fit_width() {
        val shrunk = ViewerTransform().gesture(ViewerGesture(0.5f, 500f, 300f, 0f, 0f), size)
        assertEquals(1f, shrunk.scale, 0f)
        assertEquals(0f, shrunk.x, 0f)
    }

    @Test
    fun zoom_stops_at_the_maximum_scale() {
        assertEquals(
            ViewerTransform.MAX_SCALE,
            ViewerTransform().gesture(ViewerGesture(100f, 0f, 0f, 0f, 0f), size).scale,
            0f,
        )
    }

    @Test
    fun zoom_keeps_the_point_under_the_fingers_in_place() {
        val zoomed = ViewerTransform().gesture(ViewerGesture(2f, 500f, 300f, 0f, 0f), size)
        assertEquals(-500f, zoomed.x, 0.01f)
        assertEquals(-300f, zoomed.y, 0.01f)
    }

    @Test
    fun a_pan_never_shows_past_the_pictures_edges() {
        val zoomed = ViewerTransform().gesture(ViewerGesture(2f, 500f, 300f, 0f, 0f), size)
        val right = zoomed.gesture(ViewerGesture(1f, 0f, 0f, 5000f, 5000f), size)
        assertEquals(0f, right.x, 0f)
        assertEquals(0f, right.y, 0f)
        val left = zoomed.gesture(ViewerGesture(1f, 0f, 0f, -5000f, -50000f), size)
        assertEquals(1000f - 2000f, left.x, 0.01f)
        assertEquals(600f - size.pictureHeight * 2f, left.y, 0.01f)
    }

    @Test
    fun a_picture_shorter_than_the_screen_stays_at_the_top() {
        val short = ViewerSize.fitted(1000, 200, 1000f, 600f)
        assertEquals(0f, ViewerTransform().gesture(ViewerGesture(1f, 0f, 0f, 0f, 90f), short).y, 0f)
    }

    @Test
    fun sampling_keeps_the_target_width_and_the_texture_limit() {
        assertEquals(4, ImageSampling.sample(1440, 7574, 360, 8192))
        assertEquals(1, ImageSampling.sample(1440, 7574, 1440, 8192))
        assertEquals(2, ImageSampling.sample(1440, 7574, 1440, 4096))
        assertEquals(1, ImageSampling.sample(100, 100, 360, 4096))
    }

    @Test
    fun sampling_keeps_a_viewer_picture_inside_the_pixel_budget() {
        assertEquals(1, ImageSampling.sample(1440, 7574, 2560, 8192, 16_000_000L))
        assertEquals(2, ImageSampling.sample(1440, 20_000, 2560, 32_000, 16_000_000L))
        assertEquals(4, ImageSampling.sample(1440, 7574, 2560, 8192, 1_000_000L))
    }

    @Test
    fun a_tall_picture_is_placed_at_the_top_left_so_its_top_is_reachable() {
        val tall = ViewerSize.fitted(1440, 7574, 2560f, 1600f)
        val box = tall.box()
        assertEquals(ViewerBox(0, 0, 2560, 13_465), box)
        // The first view shows the picture's top row; panning down reaches the bottom and no further.
        val start = ViewerTransform()
        assertEquals(0f, start.topRow(), 0f)
        val bottom = start.gesture(ViewerGesture(1f, 0f, 0f, 0f, -1e6f), tall)
        assertEquals(1600f - tall.pictureHeight, bottom.y, 0.01f)
        assertEquals(tall.pictureHeight - 1600f, bottom.topRow(), 0.01f)
        val back = bottom.gesture(ViewerGesture(1f, 0f, 0f, 0f, 1e6f), tall)
        assertEquals(0f, back.topRow(), 0f)
    }

    @Test
    fun a_new_screen_goes_under_the_lowest_frame_at_the_left_edge() {
        val boxes = listOf(PageRect(100f, 0f, 1440f, 900f), PageRect(0f, 2000f, 390f, 844f))
        val (x, y) = NewScreenPlacement.at(boxes)
        assertEquals(0f, x, 0f)
        assertEquals(2844f + 160f, y, 0f)
        assertEquals(0f to 0f, NewScreenPlacement.at(emptyList()))
    }
}
