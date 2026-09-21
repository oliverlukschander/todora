import datetime
import unittest
from sign_game_center import entitlements


class SigningTests(unittest.TestCase):
    def setUp(self):
        self.now = datetime.datetime(2026, 9, 21)
        self.profile = {
            "Platform": ["OSX"], "ExpirationDate": self.now + datetime.timedelta(days=30),
            "Entitlements": {
                "com.apple.application-identifier": "TEAM.com.lukschander.todora",
                "com.apple.developer.team-identifier": "TEAM",
                "com.apple.developer.game-center": True,
            },
        }

    def test_only_an_explicit_current_todora_macos_profile_is_accepted(self):
        rights = entitlements(self.profile, "com.lukschander.todora", self.now)
        self.assertTrue(rights["com.apple.security.app-sandbox"])
        self.assertTrue(rights["com.apple.security.network.server"])
        for field, value in [("Platform", ["iOS"]), ("ExpirationDate", self.now)]:
            with self.assertRaises(ValueError):
                entitlements(dict(self.profile, **{field: value}), "com.lukschander.todora", self.now)
        for key, value in [("com.apple.application-identifier", "TEAM.*"),
                           ("com.apple.developer.game-center", False),
                           ("com.apple.developer.team-identifier", "OTHER")]:
            profile = dict(self.profile, Entitlements=dict(self.profile["Entitlements"], **{key: value}))
            with self.assertRaises(ValueError):
                entitlements(profile, "com.lukschander.todora", self.now)


if __name__ == "__main__":
    unittest.main()
