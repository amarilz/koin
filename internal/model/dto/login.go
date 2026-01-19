package dto

type LoginRequestDto struct {
	Email    string `json:"email"`
	Password string `json:"password"`
}

type LoginResponseDto struct {
	UserID int64  `json:"userId"`
	Email  string `json:"email"`
}
