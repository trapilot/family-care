// Native helpers for the app: rear-camera capture and ML Kit text recognition (called from Rust over JNI)
plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.example.familymoney.camera"
    compileSdk = 34
    defaultConfig {
        minSdk = 24
        // Rust looks the class up by name: keep it in R8-minified release builds
        consumerProguardFiles("consumer-rules.pro")
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
}

dependencies {
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.exifinterface:exifinterface:1.3.7")
    // On-device text recognition, model bundled in the APK (works offline, no Play services download)
    implementation("com.google.mlkit:text-recognition:16.0.1")
}
