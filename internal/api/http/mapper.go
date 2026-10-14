package http

import (
	apigen "koin/internal/api/generated"
	"koin/internal/model/dto"
)

func ToCreateAccountDto(in *apigen.CreateAccountJSONRequestBody) dto.CreateAccountDto {
	return dto.CreateAccountDto{
		Name:           in.Name,
		Currency:       in.Currency,
		InitialBalance: in.InitialBalance,
	}
}

func ToAddExpenseDto(in *apigen.AddTransactionJSONRequestBody) dto.AddTransactionDto {
	var desc *string
	if in.Description != "" {
		desc = &in.Description
	}
	return dto.AddTransactionDto{
		AccountName:  in.AccountName,
		CategoryName: in.CategoryName,
		CategoryType: dto.CategoryType(in.CategoryType),
		OccurredAt:   in.OccurredAt.Time,
		Amount:       in.Amount,
		Description:  desc,
	}
}

func ToCreateCategoryDto(in *apigen.CreateCategoryJSONRequestBody) dto.CreateCategoryDto {
	description := ""
	if in.Description != nil {
		description = *in.Description
	}
	return dto.CreateCategoryDto{
		Name:         in.Name,
		CategoryType: dto.CategoryType(in.CategoryType),
		Description:  description,
	}
}
