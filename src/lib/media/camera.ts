/** Camera preview through getUserMedia (D6). The stream is stopped when recording starts. */
export async function openCamera(deviceLabelOrId: string | null): Promise<MediaStream | null> {
  if (typeof navigator === "undefined" || !navigator.mediaDevices?.getUserMedia) return null;
  const devices = await navigator.mediaDevices.enumerateDevices().catch(() => []);
  const cams = devices.filter((d) => d.kind === "videoinput");
  // ffmpeg ids are avfoundation indices / dshow names; match by label when we can.
  const match = cams.find((d) => d.label === deviceLabelOrId) ?? cams.find((d) => d.deviceId === deviceLabelOrId);
  const video: MediaTrackConstraints = match ? { deviceId: { exact: match.deviceId } } : { facingMode: "user" };
  try {
    return await navigator.mediaDevices.getUserMedia({ video, audio: false });
  } catch {
    return null;
  }
}

export function stopStream(stream: MediaStream | null | undefined): void {
  stream?.getTracks().forEach((t) => t.stop());
}
