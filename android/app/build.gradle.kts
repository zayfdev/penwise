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

    testOptions {
        // Log calls in code under test return defaults instead of throwing "not mocked".
        unitTests.isReturnDefaultValues = true
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

/** Builds the host (JVM) `libpenwise_ffi.so` so unit tests run the real Rust through JNI. */
abstract class CargoHostTask : DefaultTask() {
    @get:Inject
    abstract val execOperations: ExecOperations

    @get:Internal
    abstract val workspaceDir: DirectoryProperty

    @get:InputFiles
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val rustSources: ConfigurableFileCollection

    @get:Input
    abstract val cargo: Property<String>

    /** Cargo's host debug directory holding `libpenwise_ffi.so`. */
    @get:OutputDirectory
    abstract val libDir: DirectoryProperty

    @TaskAction
    fun build() {
        execOperations.exec {
            workingDir = workspaceDir.get().asFile
            // The server binary is the real peer of the sync tests.
            commandLine(cargo.get(), "build", "-p", "penwise-ffi", "-p", "penwise-server")
        }
    }
}

val repoRootDir: File = rootDir.parentFile

// Gradle daemons often miss ~/.cargo/bin on PATH; prefer the rustup-managed cargo when it exists.
val cargoBinary =
    providers
        .environmentVariable("CARGO_HOME")
        .orElse(providers.systemProperty("user.home").map { "$it/.cargo" })
        .map { home -> File(home, "bin/cargo").takeIf(File::canExecute)?.path ?: "cargo" }

val cargoHost =
    tasks.register<CargoHostTask>("cargoHost") {
        workspaceDir.set(repoRootDir)
        rustSources.from(
            fileTree(File(repoRootDir, "crates")) { exclude("**/target/**") },
            File(repoRootDir, "Cargo.toml"),
            File(repoRootDir, "Cargo.lock"),
        )
        cargo.set(cargoBinary)
        libDir.set(File(repoRootDir, "target/debug"))
    }

tasks.withType<Test>().configureEach {
    dependsOn(cargoHost)
    systemProperty("java.library.path", File(repoRootDir, "target/debug").absolutePath)
    systemProperty("penwise.server.bin", File(repoRootDir, "target/debug/penwise-server").absolutePath)
    // Native.kt's `System.loadLibrary` finds the host cdylib through `java.library.path`.
    inputs.files(cargoHost.map { it.libDir })
}

androidComponents {
    val repoRoot = repoRootDir
    val cargoPath = cargoBinary
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
    implementation(libs.androidx.compose.material.icons.extended)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.graphics.core)
    implementation(libs.androidx.input.motionprediction)
    implementation(libs.okhttp)
    testImplementation(libs.junit)
    // Real org.json for JVM tests: the android.jar stub throws instead of parsing.
    testImplementation(libs.json)
}
