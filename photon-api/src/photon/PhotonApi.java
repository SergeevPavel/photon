package photon;

public class PhotonApi {
    static {
        System.loadLibrary("photonapi");
    }

    public native void run(int port);
    public native void applyUpdates(String updates);
    public native float measureText(String text);
}
