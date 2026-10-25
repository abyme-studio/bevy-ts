package dev.bevyts.smoke;

import android.app.Activity;
import android.content.Intent;
import android.os.Bundle;
import android.os.SystemClock;
import android.util.Log;
import android.view.MotionEvent;
import android.view.SurfaceHolder;
import android.view.SurfaceView;
import dev.bevyts.Engine;
import java.io.File;
import org.json.JSONArray;
import org.json.JSONObject;

/**
 * The smoke host (bench/android-smoke/run.sh): loads filesDir/libbevy_ts_android.so, plays
 * filesDir/games/<game>, and logs every engine event and every lifecycle step under the tag
 * bevy-ts-smoke, which run.sh reads. One engine for the activity's life: a new surface, a null
 * one, pause and resume go to it, as abyme's game pane will.
 */
public final class SmokeActivity extends Activity implements SurfaceHolder.Callback {
    private static final String TAG = "bevy-ts-smoke";
    private static boolean loaded;
    private Engine engine;
    private float density;
    private long createdAt;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        density = getResources().getDisplayMetrics().density;
        File lib = new File(getFilesDir(), "libbevy_ts_android.so");
        if (!loaded) {
            long t = SystemClock.elapsedRealtime();
            try {
                Engine.load(lib.getAbsolutePath());
            } catch (Throwable e) {
                Log.e(TAG, "load failed: " + e);
                finish();
                return;
            }
            loaded = true;
            Log.i(TAG, "loaded " + lib + " version " + Engine.version() + " in " + (SystemClock.elapsedRealtime() - t) + " ms");
        }
        SurfaceView view = new SurfaceView(this);
        view.getHolder().addCallback(this);
        view.setOnTouchListener((v, e) -> {
            touch(e);
            return true;
        });
        setContentView(view);
    }

    /** run.sh ends the run with an intent holding `quit`: a back press only backgrounds a task root. */
    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        if (intent.hasExtra("quit")) {
            Log.i(TAG, "quit");
            finish();
        }
    }

    private String config() throws Exception {
        String game = getIntent().getStringExtra("game");
        JSONObject c = new JSONObject();
        c.put("game", new File(getFilesDir(), "games/" + (game == null ? "arena" : game)).getAbsolutePath());
        return c.toString();
    }

    @Override
    public void surfaceCreated(SurfaceHolder holder) {
        Log.i(TAG, "surface created");
    }

    @Override
    public void surfaceChanged(SurfaceHolder holder, int format, int width, int height) {
        Log.i(TAG, "surface " + width + "x" + height);
        if (engine == null) {
            createdAt = SystemClock.elapsedRealtime();
            try {
                engine = Engine.create(config(), holder.getSurface(), width, height, density, this::event);
            } catch (Throwable e) {
                Log.e(TAG, "create failed: " + e);
                finish();
            }
        } else {
            engine.surface(holder.getSurface(), width, height, density);
        }
    }

    @Override
    public void surfaceDestroyed(SurfaceHolder holder) {
        if (engine != null) {
            long t = SystemClock.elapsedRealtime();
            engine.surface(null, 0, 0, density);
            Log.i(TAG, "surface released in " + (SystemClock.elapsedRealtime() - t) + " ms");
        }
    }

    private void event(String json) {
        if (json.contains("\"started\"")) {
            Log.i(TAG, "first frame in " + (SystemClock.elapsedRealtime() - createdAt) + " ms");
        }
        Log.i(TAG, "event " + json);
    }

    private void touch(MotionEvent e) {
        if (engine == null) {
            return;
        }
        String kind;
        switch (e.getActionMasked()) {
            case MotionEvent.ACTION_DOWN:
            case MotionEvent.ACTION_POINTER_DOWN:
                kind = "start";
                break;
            case MotionEvent.ACTION_MOVE:
                kind = "move";
                break;
            case MotionEvent.ACTION_CANCEL:
                kind = "cancel";
                break;
            default:
                kind = "end";
        }
        try {
            // Every touch still down after the event, in points.
            JSONArray touches = new JSONArray();
            int lifted = e.getActionMasked() == MotionEvent.ACTION_UP || e.getActionMasked() == MotionEvent.ACTION_POINTER_UP ? e.getActionIndex() : -1;
            if (e.getActionMasked() != MotionEvent.ACTION_CANCEL) {
                for (int i = 0; i < e.getPointerCount(); i++) {
                    if (i == lifted) {
                        continue;
                    }
                    JSONObject t = new JSONObject();
                    t.put("id", e.getPointerId(i));
                    t.put("x", e.getX(i) / density);
                    t.put("y", e.getY(i) / density);
                    touches.put(t);
                }
            }
            JSONObject input = new JSONObject();
            input.put("type", "touch");
            input.put("kind", kind);
            input.put("touches", touches);
            engine.input(input.toString());
            if (!"move".equals(kind)) {
                Log.i(TAG, "touch " + kind);
            }
        } catch (Exception ex) {
            Log.e(TAG, "touch: " + ex);
        }
    }

    @Override
    protected void onPause() {
        super.onPause();
        if (engine != null) {
            engine.pause();
            Log.i(TAG, "paused");
        }
    }

    @Override
    protected void onResume() {
        super.onResume();
        if (engine != null) {
            engine.resume();
            Log.i(TAG, "resumed");
        }
    }

    @Override
    protected void onDestroy() {
        if (engine != null) {
            engine.close();
            engine = null;
            Log.i(TAG, "closed");
        }
        super.onDestroy();
    }
}
