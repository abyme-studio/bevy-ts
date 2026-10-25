package dev.bevyts;

import android.view.Surface;

/**
 * bevy-ts's engine, embedded: one per process, drawing into a {@link Surface} the host owns
 * (docs/android.md). The host loads a downloaded library once with {@link #load(String)}; the
 * class never loads one by name. Calls return at once, except {@link #surface} with a null
 * surface and {@link #close}, which wait for the engine to let go.
 */
public final class Engine implements AutoCloseable {
    /** The C ABI this class speaks: {@link #load} refuses a library of another. */
    public static final int ABI = 1;

    /** Events, one JSON object each: started, log, ime, exited (last). On the engine's thread. */
    public interface Listener {
        void onEvent(String json);
    }

    private long handle;

    private Engine(long handle) {
        this.handle = handle;
    }

    /**
     * Loads the engine library at {@code absolutePath} and checks its ABI; any bevy-ts version of
     * this ABI loads.
     *
     * @throws UnsatisfiedLinkError when it cannot load, or its ABI is not {@link #ABI}
     */
    public static synchronized void load(String absolutePath) {
        System.load(absolutePath);
        int abi = nativeAbi();
        if (abi != ABI) {
            throw new UnsatisfiedLinkError(absolutePath + " is engine ABI " + abi + ", but this Engine class is ABI " + ABI);
        }
    }

    /** The loaded library's bevy-ts version. */
    public static String version() {
        return nativeVersion();
    }

    /**
     * Creates the engine on {@code surface} (or none, for null). {@code width} and {@code height}
     * are physical pixels, {@code scale} pixels per point (the display's density).
     *
     * @throws IllegalStateException with the engine's error when the game cannot play
     */
    public static Engine create(String configJson, Surface surface, int width, int height, float scale, Listener listener) {
        return new Engine(nativeCreate(configJson, surface, width, height, scale, listener));
    }

    /** A new surface, the same one at a new size, or null: then waits until the engine let go. */
    public synchronized void surface(Surface surface, int width, int height, float scale) {
        nativeSurface(handle, surface, width, height, scale);
    }

    public synchronized void pause() {
        nativePause(handle);
    }

    public synchronized void resume() {
        nativeResume(handle);
    }

    /** One SURFACE.md input event, as JSON. */
    public synchronized void input(String json) {
        nativeInput(handle, json);
    }

    /** Stops the engine and waits for it; the listener has had {@code exited} by then. */
    @Override
    public synchronized void close() {
        if (handle != 0) {
            nativeDestroy(handle);
            handle = 0;
        }
    }

    private static native int nativeAbi();
    private static native String nativeVersion();
    private static native long nativeCreate(String configJson, Surface surface, int width, int height, float scale, Listener listener);
    private static native void nativeSurface(long handle, Surface surface, int width, int height, float scale);
    private static native void nativePause(long handle);
    private static native void nativeResume(long handle);
    private static native void nativeInput(long handle, String json);
    private static native void nativeDestroy(long handle);
}
