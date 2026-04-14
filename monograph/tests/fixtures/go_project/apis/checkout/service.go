package checkout

import (
    "github.com/company/enterprise-api/shared/auth"
    "fmt"
)

func Process() {
    _ = auth.ValidateToken("tok")
    fmt.Println("processed")
}
