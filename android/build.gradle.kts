plugins {
    alias(libs.plugins.android.application) apply false
    // Also pins KGP on the build classpath for AGP's built-in Kotlin.
    alias(libs.plugins.kotlin.compose) apply false
    alias(libs.plugins.ktlint) apply false
    alias(libs.plugins.detekt) apply false
}
