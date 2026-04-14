package auth

func ValidateToken(token string) bool {
    return len(token) > 0
}
