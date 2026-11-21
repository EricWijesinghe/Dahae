import java.util.Base64
import java.util.Properties

plugins {
    id("com.android.application")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

// Release signing: CI secrets (UPLOAD_KEYSTORE_* from CI secrets) or android/key.properties.
// Without either, release builds fall back to debug keys so `flutter run --release` still works.
val keyProps = Properties().apply {
    val f = rootProject.file("key.properties")
    if (f.exists()) f.inputStream().use { load(it) }
}
fun signingValue(env: String, prop: String): String? = System.getenv(env) ?: keyProps.getProperty(prop)

// Build-time --dart-define values. The Flutter tool passes them to Gradle as base64 "KEY=VALUE" entries.
val dartDefines: Map<String, String> =
    (project.findProperty("dart-defines") as String?)
        ?.split(",")
        ?.associate { entry ->
            val (key, value) = String(Base64.getDecoder().decode(entry)).split("=", limit = 2)
            key to value
        }
        .orEmpty()
// Firebase (push) needs google-services.json, written by CI from a secret. Without it the plugin stays off.
val hasFirebaseConfig = file("google-services.json").exists()
if (hasFirebaseConfig) apply(plugin = "com.google.gms.google-services")
val releaseStore = signingValue("UPLOAD_KEYSTORE_PATH", "storeFile")
// Store builds (store-apps.yml, scripts/store_brand.py) take the app id and home-screen name from the licensee's
// server and GitHub variables; local builds keep the built-in values. APP_LINK_DOMAIN may list several hosts
// (current domain first, then former ones): every host gets the Naver App Link.
fun buildEnv(name: String): String? = System.getenv(name)?.trim()?.takeIf { it.isNotEmpty() }
val appLinkHosts: List<String> =
    (buildEnv("APP_LINK_DOMAIN") ?: dartDefines["APP_LINK_DOMAIN"].orEmpty()).split(",").map { it.trim() }.filter { it.isNotEmpty() }


android {
    namespace = "kr.co.dahae.customer"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        // flutter_local_notifications needs java.time on older Android versions.
        isCoreLibraryDesugaringEnabled = true
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        applicationId = buildEnv("APP_ID") ?: "kr.co.dahae.customer"
        // You can update the following values to match your application needs.
        // For more information, see: https://flutter.dev/to/review-gradle-config.
        minSdk = flutter.minSdkVersion
        // Google Play requires API 36 for updates from 2026-08-31; pinned so an older Flutter SDK cannot lower it.
        targetSdk = 36
        // Uses the version code from pubspec.yaml. When using split APKs, 1000 * ABI_VERSION
        // is added automatically by Flutter. (https://developer.android.com/studio/build/configure-apk-splits#configure-APK-versions)
        // You can force using the value of versionCode by specifying the `-P force-version-code-ignoring-abi=true`
        // flag during build.
        versionCode = flutter.versionCode
        versionName = flutter.versionName
        // Kakao login redirect scheme is kakao<native app key>.
        manifestPlaceholders["kakaoNativeKey"] = dartDefines["KAKAO_NATIVE_KEY"].orEmpty()
        manifestPlaceholders["appLabel"] = buildEnv("APP_LABEL") ?: "다해"
        // Naver App Link hosts (https://<host>/auth/naver/callback); the Naver button is hidden without one.
        // Unused slots repeat the first host. shortcut: three hosts at most; add a slot here and in the manifest if needed.
        manifestPlaceholders["appLinkHost"] = appLinkHosts.getOrElse(0) { "" }
        manifestPlaceholders["appLinkHost2"] = appLinkHosts.getOrElse(1) { appLinkHosts.getOrElse(0) { "" } }
        manifestPlaceholders["appLinkHost3"] = appLinkHosts.getOrElse(2) { appLinkHosts.getOrElse(0) { "" } }
    }

    signingConfigs {
        if (releaseStore != null) {
            create("release") {
                storeFile = file(releaseStore)
                storePassword = signingValue("UPLOAD_KEYSTORE_PASSWORD", "storePassword")
                keyAlias = signingValue("UPLOAD_KEY_ALIAS", "keyAlias")
                keyPassword = signingValue("UPLOAD_KEY_PASSWORD", "keyPassword")
            }
        }
    }

    buildTypes {
        release {
            // GitHub Actions sets GITHUB_ACTIONS=true: a release bundle there must never fall back to the debug key.
            check(releaseStore != null || System.getenv("GITHUB_ACTIONS") == null) { "UPLOAD_KEYSTORE_PATH is unset in CI" }
            signingConfig = if (releaseStore != null) signingConfigs.getByName("release") else signingConfigs.getByName("debug")
            // Native debug symbols go into the bundle so Play Console can symbolicate native crashes and ANRs.
            ndk { debugSymbolLevel = "FULL" }
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

flutter {
    source = "../.."
}

dependencies {
    coreLibraryDesugaring("com.android.tools:desugar_jdk_libs:2.1.5")
}
