package internal

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"time"
)

const defaultMonoGraphURL = "http://127.0.0.1:7474"

// ImpactEntry is a single domain affected by the changed files.
type ImpactEntry struct {
	Domain   string  `json:"domain"`
	Risk     string  `json:"risk"`     // "critical", "high", "medium", "low"
	CallsDay int64   `json:"calls_day"`
	Score    float64 `json:"score"`
	EdgeType string  `json:"edge_type"` // "static_import" | "co_change"
}

// ImpactResponse is what /impact returns.
type ImpactResponse struct {
	Entries []ImpactEntry `json:"entries"`
}

// HasRisk returns true if any entry has the given risk level.
func (r *ImpactResponse) HasRisk(level string) bool {
	for _, e := range r.Entries {
		if e.Risk == level {
			return true
		}
	}
	return false
}

// impactRequest is the JSON body sent to POST /impact.
type impactRequest struct {
	ChangedFiles []string `json:"changed_files"`
}

// QueryImpact sends changed files to the MonoGraph /impact endpoint and
// returns the impact analysis.  The call is bounded by timeout.
//
// If the daemon is unreachable or returns an error, a non-nil error is
// returned.  Callers should treat this as advisory and let the push proceed.
func QueryImpact(baseURL string, changedFiles []string, timeout time.Duration) (*ImpactResponse, error) {
	if baseURL == "" {
		baseURL = defaultMonoGraphURL
	}

	body, err := json.Marshal(impactRequest{ChangedFiles: changedFiles})
	if err != nil {
		return nil, fmt.Errorf("marshal request: %w", err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), timeout)
	defer cancel()

	req, err := http.NewRequestWithContext(
		ctx,
		http.MethodPost,
		baseURL+"/impact",
		bytes.NewReader(body),
	)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")

	client := &http.Client{}
	resp, err := client.Do(req)
	if err != nil {
		return nil, fmt.Errorf("MonoGraph unreachable: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("MonoGraph /impact returned HTTP %d", resp.StatusCode)
	}

	var result ImpactResponse
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, fmt.Errorf("decode response: %w", err)
	}
	return &result, nil
}
