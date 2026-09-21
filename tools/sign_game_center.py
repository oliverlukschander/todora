"""Sign the prototype using an existing Todora macOS development profile."""
import datetime
import hashlib
import pathlib
import plistlib
import shutil
import subprocess
import sys
import tempfile


def entitlements(profile, bundle_id, now):
    """Reject unrelated/expired profiles before changing the app."""
    if "OSX" not in profile.get("Platform", []):
        raise ValueError("A macOS development profile is required")
    if profile["ExpirationDate"] <= now:
        raise ValueError("The development profile has expired")
    source = profile["Entitlements"]
    app_id = source.get("com.apple.application-identifier", "")
    team = source.get("com.apple.developer.team-identifier", "")
    if not team or app_id != f"{team}.{bundle_id}":
        raise ValueError(f"The profile must explicitly cover {bundle_id}")
    if not source.get("com.apple.developer.game-center"):
        raise ValueError("Enable Game Center for the App ID and regenerate the profile")
    return {
        "com.apple.application-identifier": app_id,
        "com.apple.developer.team-identifier": team,
        "com.apple.developer.game-center": True,
        "com.apple.security.app-sandbox": True,
        "com.apple.security.network.client": True,
        "com.apple.security.network.server": True,
        "com.apple.security.device.usb": True,
    }


def main():
    app, path, identity = sys.argv[1:]
    app = pathlib.Path(app)
    info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
    profile = plistlib.loads(subprocess.check_output(["security", "cms", "-D", "-i", path]))
    rights = entitlements(profile, info["CFBundleIdentifier"], datetime.datetime.now(datetime.timezone.utc).replace(tzinfo=None))
    # Compare certificate fingerprints with installed signing identities. Do
    # not accidentally use another organization's certificate from the keychain.
    authorized = {hashlib.sha1(cert).hexdigest().upper() for cert in profile.get("DeveloperCertificates", [])}
    identities = subprocess.check_output(["security", "find-identity", "-v", "-p", "codesigning"], text=True)
    matches = [fingerprint for fingerprint in authorized
               if any(fingerprint in line and identity in line for line in identities.splitlines())]
    if len(matches) != 1:
        raise ValueError("Choose one installed Apple Development identity authorized by this profile")
    identity = matches[0]
    with tempfile.TemporaryDirectory(prefix="todora-sign-") as tmp:
        plist = pathlib.Path(tmp) / "entitlements.plist"
        plist.write_bytes(plistlib.dumps(rights))
        shutil.copyfile(path, app / "Contents/embedded.provisionprofile")
        subprocess.run(["codesign", "--force", "--sign", identity, "--entitlements", str(plist), str(app)], check=True)
        subprocess.run(["codesign", "--verify", "--strict", str(app)], check=True)


if __name__ == "__main__":
    main()
