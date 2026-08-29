// Package jsonsafe validates JSON before encoding/json can replace malformed
// Unicode or collapse duplicate object fields.
package jsonsafe

import (
	"bytes"
	"encoding/json"
	"errors"
	"io"
	"unicode/utf8"
)

// Validate rejects invalid UTF-8, unpaired UTF-16 escapes, duplicate object
// fields, invalid JSON, and trailing values.
func Validate(data []byte) error {
	if !utf8.Valid(data) {
		return errors.New("invalid UTF-8")
	}
	if err := validateSurrogates(data); err != nil {
		return err
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	if err := unique(decoder); err != nil {
		return err
	}
	if _, err := decoder.Token(); err != io.EOF {
		if err != nil {
			return err
		}
		return errors.New("trailing JSON value")
	}
	return nil
}

func validateSurrogates(data []byte) error {
	for index := 0; index < len(data); index++ {
		if data[index] != '"' {
			continue
		}
		for index++; index < len(data) && data[index] != '"'; index++ {
			if data[index] != '\\' {
				continue
			}
			index++
			if index >= len(data) || data[index] != 'u' {
				continue
			}
			value, ok := hexQuad(data, index+1)
			if !ok {
				continue
			}
			index += 4
			switch {
			case value >= 0xd800 && value <= 0xdbff:
				if index+6 >= len(data) || data[index+1] != '\\' || data[index+2] != 'u' {
					return errors.New("unpaired high surrogate")
				}
				low, ok := hexQuad(data, index+3)
				if !ok || low < 0xdc00 || low > 0xdfff {
					return errors.New("unpaired high surrogate")
				}
				index += 6
			case value >= 0xdc00 && value <= 0xdfff:
				return errors.New("unpaired low surrogate")
			}
		}
	}
	return nil
}

func hexQuad(data []byte, start int) (uint16, bool) {
	if start+4 > len(data) {
		return 0, false
	}
	var value uint16
	for _, digit := range data[start : start+4] {
		value <<= 4
		switch {
		case digit >= '0' && digit <= '9':
			value |= uint16(digit - '0')
		case digit >= 'a' && digit <= 'f':
			value |= uint16(digit-'a') + 10
		case digit >= 'A' && digit <= 'F':
			value |= uint16(digit-'A') + 10
		default:
			return 0, false
		}
	}
	return value, true
}

func unique(decoder *json.Decoder) error {
	token, err := decoder.Token()
	if err != nil {
		return err
	}
	delimiter, ok := token.(json.Delim)
	if !ok {
		return nil
	}
	switch delimiter {
	case '{':
		fields := make(map[string]struct{})
		for decoder.More() {
			name, err := decoder.Token()
			if err != nil {
				return err
			}
			field, ok := name.(string)
			if !ok {
				return errors.New("invalid object field")
			}
			if _, duplicate := fields[field]; duplicate {
				return errors.New("duplicate object field")
			}
			fields[field] = struct{}{}
			if err := unique(decoder); err != nil {
				return err
			}
		}
	case '[':
		for decoder.More() {
			if err := unique(decoder); err != nil {
				return err
			}
		}
	default:
		return errors.New("invalid JSON delimiter")
	}
	_, err = decoder.Token()
	return err
}
