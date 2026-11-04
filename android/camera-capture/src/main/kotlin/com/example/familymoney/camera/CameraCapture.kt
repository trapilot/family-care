package com.example.familymoney.camera

import android.app.Activity
import android.content.ClipData
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Matrix
import android.net.Uri
import android.os.Environment
import android.provider.MediaStore
import android.util.Base64
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.FileProvider
import androidx.exifinterface.media.ExifInterface
import com.google.android.gms.tasks.Tasks
import com.google.mlkit.vision.common.InputImage
import com.google.mlkit.vision.text.TextRecognition
import com.google.mlkit.vision.text.latin.TextRecognizerOptions
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.File

/**
 * Front/rear camera capture and ML Kit text recognition, called from Rust over JNI.
 * Every public method catches Throwable: a pending Java exception would poison the caller's JNIEnv.
 */
class CameraCapture(private val activity: Activity) {
    @Volatile private var result: String? = null
    @Volatile private var pending = false
    private var launcher: ActivityResultLauncher<Intent>? = null

    /** Opens the camera app (front camera when `front` != 0, else rear). Returns 1 if started, 0 if not (Ints: Rust side can't read a JNI boolean). */
    fun capture(maxSize: Int, front: Int): Int {
        try {
            if (pending) return 0
            val probe = Intent(MediaStore.ACTION_IMAGE_CAPTURE)
            if (probe.resolveActivity(activity.packageManager) == null) return 0
            result = null
            pending = true
            activity.runOnUiThread {
                var file: File? = null
                try {
                    val created = File.createTempFile(
                        "capture_", ".jpg", activity.getExternalFilesDir(Environment.DIRECTORY_PICTURES)
                    )
                    file = created
                    val uri = FileProvider.getUriForFile(activity, activity.packageName + ".fileprovider", created)
                    val intent = Intent(MediaStore.ACTION_IMAGE_CAPTURE).apply {
                        putExtra(MediaStore.EXTRA_OUTPUT, uri)
                        addFlags(Intent.FLAG_GRANT_WRITE_URI_PERMISSION or Intent.FLAG_GRANT_READ_URI_PERMISSION)
                        // Lens hints: camera apps read different extras and may ignore them
                        if (front != 0) {
                            putExtra("android.intent.extras.CAMERA_FACING", 1)
                            putExtra("android.intent.extras.LENS_FACING_FRONT", 1)
                            putExtra("android.intent.extra.USE_FRONT_CAMERA", true)
                            putExtra("com.google.assistant.extra.USE_FRONT_CAMERA", true)
                        } else {
                            putExtra("android.intent.extras.CAMERA_FACING", 0)
                            putExtra("android.intent.extras.LENS_FACING_BACK", 1)
                            putExtra("android.intent.extra.USE_FRONT_CAMERA", false)
                        }
                    }
                    val l = (activity as ComponentActivity).activityResultRegistry.register(
                        "ff_camera_capture", ActivityResultContracts.StartActivityForResult()
                    ) { res ->
                        launcher?.unregister()
                        launcher = null
                        if (res.resultCode == Activity.RESULT_OK && created.length() > 0) {
                            Thread {
                                try {
                                    finish(encode(created, maxSize))
                                } finally {
                                    created.delete()
                                }
                            }.start()
                        } else {
                            created.delete()
                            finish("")
                        }
                    }
                    launcher = l
                    l.launch(intent)
                } catch (t: Throwable) {
                    Log.e(TAG, "capture launch failed", t)
                    file?.delete()
                    finish("")
                }
            }
            return 1
        } catch (t: Throwable) {
            Log.e(TAG, "capture failed", t)
            pending = false
            return 0
        }
    }

    /** Null while a capture is pending, else the base64 JPEG ("" = cancelled or failed). */
    fun takeResult(): String? {
        return try {
            if (pending) {
                null
            } else {
                val r = result ?: ""
                result = null
                r
            }
        } catch (t: Throwable) {
            ""
        }
    }

    /** Runs ML Kit on a base64 JPEG and returns the lines as JSON for Rust's OcrPage ("" on failure). */
    fun recognizeText(jpegBase64: String): String {
        var bitmap: Bitmap? = null
        try {
            val bytes = Base64.decode(jpegBase64, Base64.DEFAULT)
            bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size) ?: return ""
            val w = bitmap.width.toDouble()
            val h = bitmap.height.toDouble()
            val client = TextRecognition.getClient(TextRecognizerOptions.DEFAULT_OPTIONS)
            val text = try {
                Tasks.await(client.process(InputImage.fromBitmap(bitmap, 0)))
            } finally {
                client.close()
            }
            val lines = JSONArray()
            for (block in text.textBlocks) {
                for (line in block.lines) {
                    val p = line.cornerPoints
                    if (p == null || p.size != 4) continue
                    // Vision's convention: normalized 0..1, origin bottom-left
                    fun pt(i: Int) = JSONArray().put(p[i].x / w).put(1.0 - p[i].y / h)
                    lines.put(
                        JSONObject()
                            .put("text", line.text)
                            .put("confidence", line.confidence.toDouble())
                            .put("top_left", pt(0))
                            .put("top_right", pt(1))
                            .put("bottom_right", pt(2))
                            .put("bottom_left", pt(3))
                    )
                }
            }
            return JSONObject()
                .put("width", bitmap.width)
                .put("height", bitmap.height)
                .put("lines", lines)
                .toString()
        } catch (t: Throwable) {
            Log.e(TAG, "recognizeText failed", t)
            return ""
        } finally {
            bitmap?.recycle()
        }
    }

    /** Opens the system share sheet (app chooser) for a file in filesDir/backup or filesDir/share. Returns 1 if started, 0 if not. */
    fun shareFile(path: String, chooserTitle: String, mime: String): Int {
        try {
            val file = File(path)
            if (!file.isFile) return 0
            val uri = FileProvider.getUriForFile(activity, activity.packageName + ".fileprovider", file)
            val send = Intent(Intent.ACTION_SEND).apply {
                type = mime
                putExtra(Intent.EXTRA_STREAM, uri)
                putExtra(Intent.EXTRA_SUBJECT, file.name)
                clipData = ClipData.newRawUri(file.name, uri)
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
            val chooser = Intent.createChooser(send, chooserTitle)
            activity.runOnUiThread {
                try {
                    activity.startActivity(chooser)
                } catch (t: Throwable) {
                    Log.w(TAG, "shareFile failed", t)
                }
            }
            return 1
        } catch (t: Throwable) {
            Log.e(TAG, "shareFile failed", t)
            return 0
        }
    }

    /** Opens the dialer prefilled with `number` (no CALL_PHONE permission needed). Returns 1 if started, 0 if not. */
    fun dial(number: String): Int {
        try {
            val intent = Intent(Intent.ACTION_DIAL, Uri.parse("tel:" + Uri.encode(number)))
            activity.runOnUiThread {
                try {
                    activity.startActivity(intent)
                } catch (t: Throwable) {
                    Log.w(TAG, "dial failed", t)
                }
            }
            return 1
        } catch (t: Throwable) {
            Log.e(TAG, "dial failed", t)
            return 0
        }
    }

    private fun finish(value: String) {
        result = value
        pending = false
    }

    /** Decodes, rotates per EXIF, shrinks to maxSize on the long side, and returns base64 JPEG ("" on failure). */
    private fun encode(file: File, maxSize: Int): String {
        var decoded: Bitmap? = null
        var rotated: Bitmap? = null
        var scaled: Bitmap? = null
        var out: Bitmap? = null
        try {
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeFile(file.absolutePath, bounds)
            val longSide = maxOf(bounds.outWidth, bounds.outHeight)
            if (longSide <= 0) return ""
            var sample = 1
            while (longSide / (sample * 2) >= maxSize) sample *= 2
            val opts = BitmapFactory.Options().apply { inSampleSize = sample }
            decoded = BitmapFactory.decodeFile(file.absolutePath, opts) ?: return ""

            val degrees = when (
                ExifInterface(file.absolutePath).getAttributeInt(
                    ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_NORMAL
                )
            ) {
                ExifInterface.ORIENTATION_ROTATE_90 -> 90f
                ExifInterface.ORIENTATION_ROTATE_180 -> 180f
                ExifInterface.ORIENTATION_ROTATE_270 -> 270f
                else -> 0f
            }
            rotated = if (degrees != 0f) {
                Bitmap.createBitmap(
                    decoded, 0, 0, decoded.width, decoded.height, Matrix().apply { postRotate(degrees) }, true
                )
            } else {
                decoded
            }

            val long = maxOf(rotated.width, rotated.height)
            val ratio = if (long > maxSize) maxSize.toFloat() / long else 1f
            scaled = if (ratio < 1f) {
                Bitmap.createScaledBitmap(
                    rotated, maxOf(1, Math.round(rotated.width * ratio)), maxOf(1, Math.round(rotated.height * ratio)), true
                )
            } else {
                rotated
            }

            // White background, like the JS canvas path
            out = Bitmap.createBitmap(scaled.width, scaled.height, Bitmap.Config.ARGB_8888)
            val canvas = Canvas(out)
            canvas.drawColor(Color.WHITE)
            canvas.drawBitmap(scaled, 0f, 0f, null)
            val stream = ByteArrayOutputStream()
            out.compress(Bitmap.CompressFormat.JPEG, 85, stream)
            return Base64.encodeToString(stream.toByteArray(), Base64.NO_WRAP)
        } catch (t: Throwable) {
            Log.e(TAG, "encode failed", t)
            return ""
        } finally {
            out?.recycle()
            if (scaled !== rotated) scaled?.recycle()
            if (rotated !== decoded) rotated?.recycle()
            decoded?.recycle()
        }
    }

    private companion object {
        const val TAG = "FFCamera"
    }
}
