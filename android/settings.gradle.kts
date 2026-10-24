// The Android SDK location comes from the ANDROID_HOME environment variable (the justfile defaults it to
// ~/Android/Sdk). local.properties is never committed.
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode = RepositoriesMode.FAIL_ON_PROJECT_REPOS
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "penwise"
include(":app")
