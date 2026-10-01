// GameKit owns matchmaking and transport. All UI and match operations run on
// the main queue; Rust polls a bounded, synchronized queue of copied events.
#import <AppKit/AppKit.h>
#import <GameKit/GameKit.h>

static NSMutableArray<NSDictionary *> *events;
static void emit(int kind, NSData *data) {
    @synchronized ([GKLocalPlayer class]) {
        if (!events) events = [NSMutableArray array];
        if (events.count >= 256) {
            // Drop stale movement first; preserve connection/error events.
            NSUInteger index = [events indexOfObjectPassingTest:^BOOL(NSDictionary *e, NSUInteger i, BOOL *stop) {
                (void)i; (void)stop;
                return [e[@"kind"] intValue] == 3;
            }];
            [events removeObjectAtIndex:index == NSNotFound ? 0 : index];
        }
        [events addObject:@{@"kind": @(kind), @"data": data}];
    }
}
// Diagnostic trail in the sandbox container: Application Support/todora-gc.log.
// One process appends from several threads; it starts over past 256 KB.
static void gclog(NSString *format, ...) {
    va_list args; va_start(args, format);
    NSString *line = [[NSString alloc] initWithFormat:format arguments:args];
    va_end(args);
    NSURL *dir = [NSFileManager.defaultManager URLsForDirectory:NSApplicationSupportDirectory inDomains:NSUserDomainMask].firstObject;
    NSString *path = [dir.path stringByAppendingPathComponent:@"todora-gc.log"];
    NSData *full = [[NSString stringWithFormat:@"%@ %@\n", [NSDate date], line] dataUsingEncoding:NSUTF8StringEncoding];
    static NSObject *lock;
    static dispatch_once_t once;
    dispatch_once(&once, ^{ lock = [NSObject new]; });
    @synchronized (lock) {
        NSFileHandle *h = [NSFileHandle fileHandleForWritingAtPath:path];
        if (h && [h seekToEndOfFile] > 256 * 1024) { [h closeFile]; h = nil; }
        if (!h) { [full writeToFile:path atomically:NO]; return; }
        [h writeData:full]; [h closeFile];
    }
}
static void message(int kind, NSString *text) {
    gclog(@"event %d: %@", kind, text);
    emit(kind, [text dataUsingEncoding:NSUTF8StringEncoding]);
}

@interface TodoraGameCenter : NSObject <GKMatchmakerViewControllerDelegate, GKMatchDelegate, GKLocalPlayerListener>
@property GKMatch *match;
@property GKMatchmakerViewController *picker;
@property BOOL wanted;
@property BOOL announced;
@property NSUInteger generation;
@property NSUInteger cancelToken;
@property NSWindow *pickerWindow;
@property NSMutableArray<NSData *> *earlyData;
- (void)start;
- (void)authenticated;
- (void)closePickerWindow;
- (void)leave;
@end

@implementation TodoraGameCenter
- (void)leave {
    self.generation++;
    self.cancelToken++;
    self.wanted = NO;
    self.announced = NO;
    self.earlyData = [NSMutableArray array];
    self.match.delegate = nil;
    [self.match disconnect];
    self.match = nil;
    self.picker.matchmakerDelegate = nil;
    [self closePickerWindow];
    self.picker = nil;
    [[GKMatchmaker sharedMatchmaker] cancel];
}
- (void)fail:(NSString *)reason {
    gclog(@"fail: %@", reason);
    [self leave];
    message(4, reason);
}
- (void)presentPicker:(GKMatchmakerViewController *)picker {
    if (!picker) { [self fail:@"Game Center could not open matchmaking."]; return; }
    self.picker = picker;
    picker.matchmakerDelegate = self;
    // GKDialogController's modal session corrupts memory under this app's
    // event loop, so host the picker in a plain window of our own.
    [self closePickerWindow];
    NSWindow *window = [NSWindow windowWithContentViewController:picker];
    window.title = @"Game Center";
    window.releasedWhenClosed = NO;
    [window center];
    self.pickerWindow = window;
    __weak TodoraGameCenter *weak = self;
    [NSNotificationCenter.defaultCenter addObserverForName:NSWindowWillCloseNotification object:window queue:NSOperationQueue.mainQueue usingBlock:^(NSNotification *note) {
        TodoraGameCenter *strong = weak;
        if (strong && strong.pickerWindow == note.object) {
            strong.pickerWindow = nil;
            if (strong.picker) [strong matchmakerViewControllerWasCancelled:strong.picker];
        }
    }];
    [NSApp activateIgnoringOtherApps:YES];
    [window makeKeyAndOrderFront:nil];
}
- (void)closePickerWindow {
    NSWindow *window = self.pickerWindow;
    self.pickerWindow = nil;
    [window close];
}
// GameKit only delivers invitations to listeners registered after sign-in.
- (void)authenticated {
    gclog(@"authenticated as %@", GKLocalPlayer.localPlayer.gamePlayerID);
    [GKLocalPlayer.localPlayer unregisterAllListeners];
    [GKLocalPlayer.localPlayer registerListener:self];
}
- (void)findPlayers {
    if (!self.wanted || self.picker || self.match) return;
    if (GKLocalPlayer.localPlayer.isMultiplayerGamingRestricted) {
        [self fail:@"Multiplayer is restricted for this Game Center account."];
        return;
    }
    GKMatchRequest *request = [GKMatchRequest new];
    request.minPlayers = 2;
    request.maxPlayers = 2;
    request.defaultNumberOfPlayers = 2;
    // Separate this prototype's protocol from future incompatible versions.
    request.playerGroup = 1001;
    [self presentPicker:[[GKMatchmakerViewController alloc] initWithMatchRequest:request]];
}
- (void)start {
    [self leave];
    self.wanted = YES;
    NSString *profile = [NSBundle.mainBundle.bundlePath stringByAppendingPathComponent:@"Contents/embedded.provisionprofile"];
    if (![NSFileManager.defaultManager fileExistsAtPath:profile]) {
        [self fail:@"Game Center is not configured for this development build yet."];
        return;
    }
    NSUInteger generation = self.generation;
    GKLocalPlayer *player = GKLocalPlayer.localPlayer;
    if (player.isAuthenticated) { [self authenticated]; [self findPlayers]; return; }
    message(1, @"Signing in to Game Center…");
    player.authenticateHandler = ^(NSViewController *controller, NSError *error) {
        dispatch_async(dispatch_get_main_queue(), ^{
            if (!self.wanted || generation != self.generation) return;
            if (controller) {
                GKDialogController *dialog = [GKDialogController sharedDialogController];
                dialog.parentWindow = NSApp.keyWindow ?: NSApp.mainWindow;
                if (![dialog presentViewController:(NSViewController<GKViewController> *)controller])
                    [self fail:@"Open Game Center in System Settings and sign in, then try again."];
            } else if (GKLocalPlayer.localPlayer.isAuthenticated) {
                [self authenticated];
                [self findPlayers];
            } else {
                [self fail:error.localizedDescription ?: @"Sign in to Game Center to play online."];
            }
        });
    };
}
- (void)announce {
    if (self.announced || !self.match || self.match.expectedPlayerCount != 0) return;
    if (self.match.players.count != 1) { [self fail:@"This prototype supports two players."]; return; }
    GKPlayer *peer = self.match.players.firstObject;
    NSString *ours = GKLocalPlayer.localPlayer.gamePlayerID;
    if (!ours.length || !peer.gamePlayerID.length || [ours isEqualToString:peer.gamePlayerID]) {
        [self fail:@"Use a different Game Center account on each Mac."]; return;
    }
    self.announced = YES;
    // Both devices independently choose the same coordinator.
    uint8_t host = [ours compare:peer.gamePlayerID] == NSOrderedAscending;
    NSMutableData *data = [NSMutableData dataWithBytes:&host length:1];
    [data appendData:[peer.displayName dataUsingEncoding:NSUTF8StringEncoding]];
    emit(2, data);
    for (NSData *packet in self.earlyData) emit(3, packet);
    [self.earlyData removeAllObjects];
}
- (void)matchmakerViewController:(GKMatchmakerViewController *)picker didFindMatch:(GKMatch *)match {
    gclog(@"didFindMatch current=%d expected=%lu", picker == self.picker, (unsigned long)match.expectedPlayerCount);
    if (picker != self.picker || !self.wanted) { [match disconnect]; return; }
    // Close after GameKit finishes its own callback.
    self.picker = nil;
    picker.matchmakerDelegate = nil;
    dispatch_async(dispatch_get_main_queue(), ^{ [self closePickerWindow]; });
    self.match = match;
    match.delegate = self;
    [self announce];
}
- (void)matchmakerViewControllerWasCancelled:(GKMatchmakerViewController *)picker {
    gclog(@"wasCancelled current=%d", picker == self.picker);
    if (picker != self.picker) return;
    [self closePickerWindow];
    // GameKit cancels the open picker just before delivering an accepted
    // invitation, so give didAcceptInvite a moment to take over.
    self.picker = nil;
    NSUInteger token = ++self.cancelToken;
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(1.5 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        if (token == self.cancelToken && !self.picker && !self.match) [self fail:@"Matchmaking cancelled."];
    });
}
- (void)matchmakerViewController:(GKMatchmakerViewController *)picker didFailWithError:(NSError *)error {
    if (picker == self.picker) { [self closePickerWindow]; [self fail:error.localizedDescription]; }
}
- (void)match:(GKMatch *)match player:(GKPlayer *)player didChangeConnectionState:(GKPlayerConnectionState)state {
    gclog(@"connectionState %ld", (long)state);
    dispatch_async(dispatch_get_main_queue(), ^{
        if (match != self.match) return;
        if (state == GKPlayerStateDisconnected) [self fail:@"The other driver disconnected."];
        else [self announce];
    });
}
- (void)match:(GKMatch *)match didReceiveData:(NSData *)data fromRemotePlayer:(GKPlayer *)player {
    if (data.length > 2048) return;
    dispatch_async(dispatch_get_main_queue(), ^{
        if (match != self.match) return;
        if (self.announced) emit(3, data);
        else if (self.earlyData.count < 32) [self.earlyData addObject:data];
    });
}
- (void)match:(GKMatch *)match didFailWithError:(NSError *)error {
    dispatch_async(dispatch_get_main_queue(), ^{
        if (match == self.match) [self fail:error.localizedDescription ?: @"Game Center connection failed."];
    });
}
- (void)player:(GKPlayer *)player didAcceptInvite:(GKInvite *)invite {
    gclog(@"didAcceptInvite wanted=%d match=%d picker=%d", self.wanted, self.match != nil, self.picker != nil);
    dispatch_async(dispatch_get_main_queue(), ^{
        // Accept invitations only while the player has opened multiplayer.
        if (self.wanted && !self.match) {
            self.cancelToken++;
            // Detach first: dismissing reports a cancel that would end the session.
            GKMatchmakerViewController *old = self.picker;
            self.picker = nil;
            old.matchmakerDelegate = nil;
            [self closePickerWindow];
            [self presentPicker:[[GKMatchmakerViewController alloc] initWithInvite:invite]];
        }
    });
}
@end

static TodoraGameCenter *bridge;
void todora_gc_start(void) {
    dispatch_async(dispatch_get_main_queue(), ^{
        if (!bridge) {
            bridge = [TodoraGameCenter new];
        }
        [bridge start];
    });
}
void todora_gc_leave(void) {
    dispatch_async(dispatch_get_main_queue(), ^{ [bridge leave]; });
}
void todora_gc_send(const uint8_t *bytes, size_t length, bool reliable) {
    @autoreleasepool {
    if (length > 2048) return;
    NSData *data = [NSData dataWithBytes:bytes length:length];
    dispatch_async(dispatch_get_main_queue(), ^{
        if (!bridge.match) return;
        NSError *error = nil;
        static BOOL failing;
        if ([bridge.match sendDataToAllPlayers:data withDataMode:reliable ? GKMatchSendDataReliable : GKMatchSendDataUnreliable error:&error]) {
            failing = NO;
        } else if (reliable) {
            [bridge fail:error.localizedDescription ?: @"Could not send multiplayer data."];
        } else {
            // A lost position update is nothing; GameKit reports a dead link itself.
            if (!failing) gclog(@"unreliable send failed: %@", error.localizedDescription);
            failing = YES;
        }
    });
    }
}
void todora_gc_log(const uint8_t *bytes, size_t length) {
    @autoreleasepool {
        NSString *line = [[NSString alloc] initWithBytes:bytes length:length encoding:NSUTF8StringEncoding];
        if (line) gclog(@"%@", line);
    }
}
// Returns zero when empty, otherwise an event kind and a bounded payload.
int todora_gc_poll(uint8_t *bytes, size_t capacity, size_t *length) {
    @autoreleasepool {
    @synchronized ([GKLocalPlayer class]) {
        if (!events.count) return 0;
        NSDictionary *event = events.firstObject;
        NSData *data = event[@"data"];
        *length = MIN(capacity, data.length);
        memcpy(bytes, data.bytes, *length);
        int kind = [event[@"kind"] intValue];
        [events removeObjectAtIndex:0];
        return kind;
    }
    }
}

size_t todora_gc_support_directory(uint8_t *bytes, size_t capacity) {
    @autoreleasepool {
        NSURL *url = [NSFileManager.defaultManager URLsForDirectory:NSApplicationSupportDirectory inDomains:NSUserDomainMask].firstObject;
        NSData *path = [url.path dataUsingEncoding:NSUTF8StringEncoding];
        if (!path || path.length > capacity) return 0;
        memcpy(bytes, path.bytes, path.length);
        return path.length;
    }
}
