import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// What a sheet says about an error it caught: the same words the sheets
/// wrote by hand, whatever kind the error is. The codes' own headlines are
/// `ErrorMessagesTests`, in `AppStoreTests.swift`.
struct ErrorHeadlineTests {
    private nonisolated struct Unknown: LocalizedError {
        var errorDescription: String? { "The disk is full" }
    }

    @Test("A server error reads as its code's headline, with the server's own words under it")
    func serverError() {
        let error = ServerError(status: 403, code: "forbidden", message: "viewers cannot push")
        #expect(ErrorMessages.headline(for: error) == String(localized: "You are not allowed to do that"))
        #expect(ErrorMessages.headline(for: error) == error.summary)
        #expect(ErrorMessages.detail(for: error) == "viewers cannot push")
        #expect(ErrorMessages.line(for: error) == "\(error.summary): viewers cannot push")
    }

    @Test("A rate limit says when to try again and nothing under it")
    func rateLimit() {
        let error = ServerError(status: 429, code: "too_many_requests", message: "retry in 120 seconds", retryAfter: 120)
        #expect(ErrorMessages.headline(for: error) == ErrorMessages.tooManyAttempts(retryAfter: 120))
        #expect(ErrorMessages.detail(for: error) == nil)
        #expect(ErrorMessages.line(for: error) == ErrorMessages.tooManyAttempts(retryAfter: 120))
    }

    @Test("A domain error reads as its code's headline, with the core's message under it")
    func domainError() {
        let error = DomainError.InsufficientFunds(message: "balance 10, needs 25")
        #expect(ErrorMessages.headline(for: error) == String(localized: "Not enough money"))
        #expect(ErrorMessages.detail(for: error) == "balance 10, needs 25")
        #expect(ErrorMessages.line(for: error) == "\(String(localized: "Not enough money")): balance 10, needs 25")
    }

    @Test("Any other error is its own description, with nothing under it")
    func unknownError() {
        #expect(ErrorMessages.headline(for: Unknown()) == "The disk is full")
        #expect(ErrorMessages.detail(for: Unknown()) == nil)
        #expect(ErrorMessages.line(for: Unknown()) == "The disk is full")
    }
}
