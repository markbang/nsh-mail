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
        ndk { abiFilters += listOf("arm64-v8a", "armeabi-v7a") }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
}

dependencies {
    // prepare.sh writes it: nsh.aar with the libnsh.so nsh builds for the app's crate.
    implementation(files("libs/nsh-mail.aar"))
    implementation(platform("com.google.firebase:firebase-bom:33.7.0"))
    implementation("com.google.firebase:firebase-messaging")
}
