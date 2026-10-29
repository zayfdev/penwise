package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Where a frame's page loads from (C2): the project's port on the sync host, token-entered off loopback. */
class FrameUrlsTest {
    @Test
    fun loopback_loads_the_route_directly() {
        assertEquals(
            "http://127.0.0.1:17423/about",
            FrameUrls.frameUrl("http://127.0.0.1:7420", 17423, "/about", "secret"),
        )
        assertEquals(
            "http://localhost:17423/",
            FrameUrls.frameUrl("http://localhost:7420", 17423, "/", "secret"),
        )
    }

    @Test
    fun another_machine_enters_through_the_token_first() {
        // The token is required off-loopback and a page load cannot carry a header, so the enter endpoint sets the
        // cookie and redirects to the route.
        assertEquals(
            "http://192.168.1.19:17423/__penwise/enter?token=secret&to=%2Fabout%3Fx%3D1",
            FrameUrls.frameUrl("http://192.168.1.19:7420", 17423, "/about?x=1", "secret"),
        )
    }

    @Test
    fun a_token_with_special_characters_is_encoded() {
        assertEquals(
            "http://100.64.10.100:17423/__penwise/enter?token=a%2Bb%26c&to=%2F",
            FrameUrls.frameUrl("http://100.64.10.100:7420", 17423, "/", "a+b&c"),
        )
    }

    @Test
    fun the_host_keeps_its_brackets_when_it_is_ipv6() {
        assertEquals(
            "http://[::1]:17423/work",
            FrameUrls.frameUrl("http://[::1]:7420", 17423, "/work", "secret"),
        )
        assertEquals("[::1]", FrameUrls.hostOf("http://[::1]:7420"))
        assertTrue(FrameUrls.isLoopback("[::1]"))
    }

    @Test
    fun a_missing_base_builds_nothing() {
        assertNull(FrameUrls.frameUrl(null, 17423, "/", "secret"))
        assertNull(FrameUrls.frameUrl("", 17423, "/", "secret"))
        assertNull(FrameUrls.frameUrl("http://", 17423, "/", "secret"))
    }

    @Test
    fun hosts_are_classified() {
        assertTrue(FrameUrls.isLoopback("127.0.0.1"))
        assertTrue(FrameUrls.isLoopback("127.1.2.3"))
        assertTrue(FrameUrls.isLoopback("localhost"))
        assertFalse(FrameUrls.isLoopback("192.168.1.19"))
        assertFalse(FrameUrls.isLoopback("100.64.10.100"))
        assertEquals("192.168.1.19", FrameUrls.hostOf("http://192.168.1.19:7420"))
        assertEquals("host", FrameUrls.hostOf("https://host/"))
        assertNull(FrameUrls.hostOf("host:7420"))
    }

    @Test
    fun origin_is_what_decides_a_reload() {
        assertEquals(
            "http://192.168.1.19:17423",
            FrameUrls.origin("http://192.168.1.19:17423/__penwise/enter?token=t&to=%2F"),
        )
        assertEquals("http://127.0.0.1:17423", FrameUrls.origin("http://127.0.0.1:17423/about"))
        assertNull(FrameUrls.origin(null))
        assertNull(FrameUrls.origin("about"))
    }

    @Test
    fun routes_are_normalized_for_the_item_field() {
        assertEquals("/", FrameRoutes.normalize(""))
        assertEquals("/", FrameRoutes.normalize("  "))
        assertEquals("/about", FrameRoutes.normalize("about"))
        assertEquals("/about", FrameRoutes.normalize("/about"))
        assertEquals("/work?x=1", FrameRoutes.normalize("http://127.0.0.1:17423/work?x=1"))
        assertEquals("/", FrameRoutes.normalize("http://127.0.0.1:17423"))
        assertNull(FrameRoutes.normalize("/" + "a".repeat(2048)))
    }

    @Test
    fun a_loaded_url_yields_its_route() {
        assertEquals("/about?x=1", FrameRoutes.ofUrl("http://127.0.0.1:17423/about?x=1"))
        assertEquals("/", FrameRoutes.ofUrl("http://127.0.0.1:17423/"))
        assertEquals("/", FrameRoutes.ofUrl("http://127.0.0.1:17423"))
        assertNull(FrameRoutes.ofUrl("about"))
    }
}
