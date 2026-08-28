package api

import (
	"encoding/json"
	"testing"
)

func TestEnumJSONRepresentations(t *testing.T) {
	tests := []struct {
		name  string
		value any
		want  string
	}{
		{"challenge registration", ChallengePurposeRegistration, `"registration"`},
		{"challenge session", ChallengePurposeSession, `"session"`},
		{"token type", TokenTypeDPoP, `"DPoP"`},
		{"letter waiting", LetterStateWaiting, `"waiting"`},
		{"letter claimed", LetterStateClaimed, `"claimed"`},
		{"letter opened", LetterStateOpened, `"opened"`},
		{"letter replied", LetterStateReplied, `"replied"`},
		{"letter withdrawn", LetterStateWithdrawn, `"withdrawn"`},
		{"letter reported", LetterStateReported, `"reported"`},
		{"sender role", LetterRoleSender, `"sender"`},
		{"recipient role", LetterRoleRecipient, `"recipient"`},
		{"original target", ReportTargetOriginal, `"original"`},
		{"reply target", ReportTargetReply, `"reply"`},
		{"harassment", ReportReasonHarassment, `"harassment"`},
		{"hateful content", ReportReasonHatefulContent, `"hateful_content"`},
		{"sexual content", ReportReasonSexualContent, `"sexual_content"`},
		{"threats", ReportReasonThreats, `"threats"`},
		{"spam or scams", ReportReasonSpamOrScams, `"spam_or_scams"`},
		{"personal information", ReportReasonExposedPersonalInformation, `"exposed_personal_information"`},
		{"other unsafe content", ReportReasonOtherUnsafeContent, `"other_unsafe_content"`},
		{"no action", ModerationDispositionNoAction, `"no_action"`},
		{"duplicate", ModerationDispositionDuplicate, `"duplicate"`},
		{"identity disabled", ModerationDispositionIdentityDisabled, `"identity_disabled"`},
		{"moderation purpose", ModerationPurposeReportedContentReview, `"reported-content-review"`},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got, err := json.Marshal(test.value)
			if err != nil {
				t.Fatal(err)
			}
			if string(got) != test.want {
				t.Fatalf("json = %s, want %s", got, test.want)
			}
		})
	}
}

func TestErrorResponseShape(t *testing.T) {
	response := ErrorResponse{Error: APIError{
		Code:    ErrorCodeLetterAlreadyReplied,
		Message: "This letter already has a reply.",
	}}
	got, err := json.Marshal(response)
	if err != nil {
		t.Fatal(err)
	}
	want := `{"error":{"code":"letter_already_replied","message":"This letter already has a reply."}}`
	if string(got) != want {
		t.Fatalf("json = %s, want %s", got, want)
	}
}
