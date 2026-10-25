import org.gradle.process.ExecOperations
import javax.inject.Inject

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.ktlint)
    alias(libs.plugins.detekt)
}

android {
    namespace = "dev.penwise"
    compileSdk {
        version = release(37)
    }
    buildToolsVersion = "37.0.0"
    ndkVersion = "30.0.16248370"

    defaultConfig {
        applicationId = "dev.penwise"
        minSdk = 34
        // Deliberately below compileSdk: targetSdk 37 enables Local Network Protection (see lint.xml).
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"

        ndk {
            abiFilters += "arm64-v8a"
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_21
        targetCompatibility = JavaVersion.VERSION_21
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    lint {
        warningsAsErrors = true
        abortOnError = true
        checkDependencies = true
        checkReleaseBuilds = true
        lintConfig = file("lint.xml")
    }
}

/** Builds `libpenwise_ffi.so` (crates/penwise-ffi) with cargo-ndk into [outDir] (jniLibs layout: `arm64-v8a/`). */
abstract class CargoNdkTask : DefaultTask() {
    @get:Inject
    abstract val execOperations: ExecOperations

    /** Cargo workspace root (the repository root). */
    @get:Internal
    abstract val workspaceDir: DirectoryProperty

    @get:InputFiles
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val rustSources: ConfigurableFileCollection

    @get:Input
    abstract val cargo: Property<String>

    @get:Internal
    abstract val ndkDir: DirectoryProperty

    @get:OutputDirectory
    abstract val outDir: DirectoryProperty

    @TaskAction
    fun build() {
        val out = outDir.get().asFile
        out.deleteRecursively()
        execOperations.exec {
            workingDir = workspaceDir.get().asFile
            environment("ANDROID_NDK_HOME", ndkDir.get().asFile.absolutePath)
            commandLine(
                cargo.get(),
                "ndk",
                "-t",
                "arm64-v8a",
                "-P",
                "34",
                "-o",
                out.absolutePath,
                "build",
                "--release",
                "-p",
                "penwise-ffi",
            )
        }
    }
}

androidComponents {
    val repoRoot = rootDir.parentFile
    // Gradle daemons often miss ~/.cargo/bin on PATH; prefer the rustup-managed cargo when it exists.
    val cargoPath =
        providers
            .environmentVariable("CARGO_HOME")
            .orElse(providers.systemProperty("user.home").map { "$it/.cargo" })
            .map { home -> File(home, "bin/cargo").takeIf(File::canExecute)?.path ?: "cargo" }
    val ndkDirectory = sdkComponents.ndkDirectory
    onVariants { variant ->
        val cargoNdk =
            tasks.register<CargoNdkTask>("cargoNdk${variant.name.replaceFirstChar { it.uppercase() }}") {
                workspaceDir.set(repoRoot)
                rustSources.from(
                    fileTree(File(repoRoot, "crates")) { exclude("**/target/**") },
                    File(repoRoot, "Cargo.toml"),
                    File(repoRoot, "Cargo.lock"),
                )
                cargo.set(cargoPath)
                ndkDir.set(ndkDirectory)
            }
        variant.sources.jniLibs?.addGeneratedSourceDirectory(cargoNdk, CargoNdkTask::outDir)
    }
}

kotlin {
    jvmToolchain(21)
}

ktlint {
    version = libs.versions.ktlint
}

detekt {
    buildUponDefaultConfig = true
    config.setFrom(rootProject.file("config/detekt/detekt.yml"))
}

dependencies {
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.foundation)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.graphics.core)
    implementation(libs.androidx.input.motionprediction)
    implementation(libs.okhttp)
    testImplementation(libs.junit)
    // Real org.json for JVM tests: the android.jar stub throws instead of parsing.
    testImplementation(libs.json)
}
