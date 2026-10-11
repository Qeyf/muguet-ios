import XCTest

final class LauncherUITests: XCTestCase {
    @MainActor
    func testLauncherSurvivesSceneReactivation() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()

        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 30))
        assertLauncherVisible(app)
        attachScreenshot(app, named: "Launcher after cold launch")

        XCUIDevice.shared.press(.home)
        XCTAssertTrue(app.wait(for: .runningBackground, timeout: 5) || app.state == .runningBackgroundSuspended)
        app.activate()

        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 15))
        assertLauncherVisible(app)
        attachScreenshot(app, named: "Launcher after scene reactivation")
    }

    @MainActor
    private func assertLauncherVisible(_ app: XCUIApplication) {
        let title = app.staticTexts["Muguet"]
        XCTAssertTrue(title.waitForExistence(timeout: 20))
        XCTAssertTrue(title.isHittable, "The launcher title must be visible")
        let chooseZIP = app.buttons["Choose game ZIP…"]
        XCTAssertTrue(chooseZIP.waitForExistence(timeout: 20))
        XCTAssertTrue(chooseZIP.isHittable, "The ZIP import button must be visible")
        let play = app.buttons["Play"]
        XCTAssertTrue(play.exists)
        XCTAssertFalse(play.isEnabled, "A launcher without embedded game code must not enable Play")
    }

    @MainActor
    private func attachScreenshot(_ app: XCUIApplication, named name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
