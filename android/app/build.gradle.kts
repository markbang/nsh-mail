import com.android.build.api.variant.FilterConfiguration

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

// Without a Firebase config the APK still builds and runs; it just gets no FCM token.
if (file("google-services.json").exists()) {
    apply(plugin = "com.google.gms.google-services")
}

// nsh.json stays the one source of the app's id and version.
val nsh = groovy.json.JsonSlurper().parse(rootProject.file("../app/nsh.json")) as Map<*, *>

android {
    namespace = "top.bangwu.nshmail"
    compileSdk = 35

    defaultConfig {
        applicationId = nsh["id"] as String
        minSdk = 26
        targetSdk = 35
        // The universal APK's and the AAB's code; each ABI's APK adds its digit below.
        versionCode = (nsh["build"] as Number).toInt() * 10
        versionName = nsh["version"] as String
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }

    // One APK per ABI prepare.sh built (same NSH_ABIS and default); the AAB keeps them all and Play splits it.
    splits {
        abi {
            isEnable = true
            reset()
            include(*(System.getenv("NSH_ABIS") ?: "arm64-v8a").split(",").toTypedArray())
            isUniversalApk = false
        }
    }

    // The upload key CI decodes from its secrets (app.yml), with the one password `nsh package --keystore` takes too.
    val keystore = System.getenv("ANDROID_KEYSTORE_FILE")
    if (keystore != null) {
        signingConfigs.create("release") {
            storeFile = file(keystore)
            storePassword = System.getenv("NSH_KEYSTORE_PASSWORD")
            keyAlias = System.getenv("ANDROID_KEY_ALIAS")
            keyPassword = System.getenv("NSH_KEYSTORE_PASSWORD")
        }
        buildTypes.getByName("release").signingConfig = signingConfigs.getByName("release")
    }
}

// versionCode = build × 10 + the ABI's digit, as for the release APKs (android/README.md, Version codes).
val abiDigits = mapOf("armeabi-v7a" to 1, "arm64-v8a" to 2, "x86_64" to 3)
androidComponents {
    onVariants { variant ->
        variant.outputs.forEach { output ->
            val abi = output.filters.find { it.filterType == FilterConfiguration.FilterType.ABI }?.identifier
            output.versionCode.set((nsh["build"] as Number).toInt() * 10 + (abiDigits[abi] ?: 0))
        }
    }
}

dependencies {
    // prepare.sh writes it: nsh.aar with the libnsh.so nsh builds for the app's crate.
    implementation(files("libs/nsh-mail.aar"))
    implementation(platform("com.google.firebase:firebase-bom:33.7.0"))
    implementation("com.google.firebase:firebase-messaging")
}
