package dev.penwise.frames

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Color
import android.view.View
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import androidx.core.view.doOnLayout
import dev.penwise.BuildConfig

/**
 * Builds a frame's page view (§6.4): a WebView laid out at the preset converted to device px, so its CSS viewport is
 * the preset width and its breakpoints never see the canvas zoom; the host's bridge entry point posts the page's
 * messages to [sink] (the frame layer's delivery).
 */
internal object FrameViews {
    /** The page of [spec] at [pixels], already loading its URL. */
    @SuppressLint("SetJavaScriptEnabled")
    fun open(
        context: Context,
        spec: FrameSpec,
        pixels: PagePixels,
        sink: (FrameMessage) -> Unit,
    ): FrameWebView {
        val view = FrameWebView(context)
        view.viewportMeta = pixels.viewportMeta(spec.cssWidth)
        with(view.settings) {
            javaScriptEnabled = true
            domStorageEnabled = true
            // The page is a document, not an app: no pinch inside it, no zoom buttons; the canvas owns the zoom.
            setSupportZoom(false)
            builtInZoomControls = false
            displayZoomControls = false
            // The viewport <meta> decides the layout width: the bridge sets it to the preset under the view's zoom.
            useWideViewPort = true
            loadWithOverviewMode = false
            mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
        }
        view.setBackgroundColor(Color.WHITE)
        // The page owns its edges: no glow or stretch where the window meets the end of the document.
        view.overScrollMode = View.OVER_SCROLL_NEVER
        view.isVerticalScrollBarEnabled = false
        view.isHorizontalScrollBarEnabled = false
        view.webViewClient = WebViewClient()
        view.addJavascriptInterface(FrameBridge(sink) { view.viewportMeta }, FrameMessages.HOST)
        view.layoutParams = FrameLayout.LayoutParams(pixels.width, pixels.height)
        view.pivotX = 0f
        view.pivotY = 0f
        if (BuildConfig.DEBUG) WebView.setWebContentsDebuggingEnabled(true)
        // Loading before the view has its size lays the page out at Chromium's 980 px default first; a page that
        // sizes itself from its own scroll height (the portfolio's spine) then keeps the taller height for good.
        view.doOnLayout { if (view.url == null) view.loadUrl(spec.url) }
        return view
    }
}
