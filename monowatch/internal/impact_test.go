package internal

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestImpactResponse_HasRisk(t *testing.T) {
	resp := &ImpactResponse{
		Entries: []ImpactEntry{
			{Domain: "apis/payments", Risk: "critical"},
			{Domain: "apis/sales", Risk: "high"},
			{Domain: "shared/types", Risk: "low"},
		},
	}

	if !resp.HasRisk("critical") {
		t.Error("HasRisk(critical) = false, want true")
	}
	if !resp.HasRisk("high") {
		t.Error("HasRisk(high) = false, want true")
	}
	if resp.HasRisk("medium") {
		t.Error("HasRisk(medium) = true, want false")
	}
}

func TestQueryImpact_OK(t *testing.T) {
	want := ImpactResponse{
		Entries: []ImpactEntry{
			{Domain: "apis/payments", Risk: "critical", CallsDay: 1_000_000, Score: 0.95},
		},
	}

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost || r.URL.Path != "/impact" {
			http.NotFound(w, r)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		_ = json.NewEncoder(w).Encode(want)
	}))
	defer srv.Close()

	got, err := QueryImpact(srv.URL, []string{"shared/auth/jwt.go"}, 5*time.Second)
	if err != nil {
		t.Fatalf("QueryImpact error = %v", err)
	}
	if len(got.Entries) != 1 {
		t.Fatalf("got %d entries, want 1", len(got.Entries))
	}
	if got.Entries[0].Risk != "critical" {
		t.Errorf("Risk = %q, want critical", got.Entries[0].Risk)
	}
}

func TestQueryImpact_ServerError(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
	}))
	defer srv.Close()

	_, err := QueryImpact(srv.URL, []string{"x"}, 5*time.Second)
	if err == nil {
		t.Error("QueryImpact should return error on 500 response")
	}
}

func TestQueryImpact_Unreachable(t *testing.T) {
	// Nothing listening on port 19996.
	_, err := QueryImpact("http://127.0.0.1:19996", []string{"x"}, 500*time.Millisecond)
	if err == nil {
		t.Error("QueryImpact should return error when daemon is unreachable")
	}
}

func TestQueryImpact_Timeout(t *testing.T) {
	// Server that never responds.
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Block until the client gives up.
		select {}
	}))
	defer srv.Close()

	_, err := QueryImpact(srv.URL, []string{"x"}, 100*time.Millisecond)
	if err == nil {
		t.Error("QueryImpact should return error on timeout")
	}
}
