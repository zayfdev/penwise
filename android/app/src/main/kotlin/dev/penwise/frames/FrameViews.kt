package dev.penwise.frames

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Color
import android.view.View
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import dev.penwise.BuildConfig

/**
 * Builds a frame's page view (§6.4): a WebView laid out at the preset converted to device px, so its CSS viewport is
 * the preset width and its breakpoints never see the canvas zoom; the host's bridge entry point posts the page's
 * messages to [sink] (the frame layer's delivery).
 */
internal object FrameViews {
    /** The page of [spec] at [density], already loading its URL. */
    @SuppressLint("SetJavaScriptEnabled")
    fun open(
        context: Context,
        spec: FrameSpec,
        density: Float,
        sink: (FrameMessage) -> Unit,
    ): FrameWebView {
        val view = FrameWebView(context)
        with(view.settings) {
            javaScriptEnabled = true
            domStorageEnabled = true
            // The page is a document, not an app: no pinch inside it, no zoom buttons; the canvas owns the zoom.
            setSupportZoom(false)
            builtInZoomControls = false
            displayZoomControls = false
            // No useWideViewPort: the layout width is the view's width, so the page sees the preset as its viewport.
            useWideViewPort = false
            loadWithOverviewMode = false
            mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
        }
        view.setBackgroundColor(Color.WHITE)
        // The page owns its edges: no glow or stretch where the window meets the end of the document.
        view.overScrollMode = View.OVER_SCROLL_NEVER
        view.isVerticalScrollBarEnabled = false
        view.isHorizontalScrollBarEnabled = false
        view.webViewClient = WebViewClient()
        view.addJavascriptInterface(FrameBridge(sink), FrameMessages.HOST)
        view.layoutParams =
            FrameLayout.LayoutParams((spec.cssWidth * density).toInt(), (spec.cssHeight * density).toInt())
        view.pivotX = 0f
        view.pivotY = 0f
        if (BuildConfig.DEBUG) WebView.setWebContentsDebuggingEnabled(true)
        view.loadUrl(spec.url)
        return view
    }
}
