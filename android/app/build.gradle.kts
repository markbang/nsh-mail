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
        versionCode = (nsh["build"] as Number).toInt()
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

dependencies {
    // prepare.sh writes it: nsh.aar with the libnsh.so nsh builds for the app's crate.
    implementation(files("libs/nsh-mail.aar"))
    implementation(platform("com.google.firebase:firebase-bom:33.7.0"))
    implementation("com.google.firebase:firebase-messaging")
}
