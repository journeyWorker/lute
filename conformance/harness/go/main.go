package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"math"
	"os"
	"strings"

	"cel.dev/cel-go/cel"
	"cel.dev/cel-go/common/functions"
	"cel.dev/cel-go/common/types"
	"cel.dev/cel-go/common/types/ref"
	"cel.dev/cel-go/common/types/traits"
)

type TV struct {
	Kind string
	I    int64
	D    float64
	B    bool
	S    string
	L    []TV
	M    map[string]TV
}

func (v *TV) UnmarshalJSON(b []byte) error {
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(b, &raw); err != nil {
		return err
	}
	for k, x := range raw {
		v.Kind = k
		switch k {
		case "int":
			return json.Unmarshal(x, &v.I)
		case "double":
			var z any
			if err := json.Unmarshal(x, &z); err != nil {
				return err
			}
			switch q := z.(type) {
			case float64:
				v.D = q
			case string:
				if q == "inf" {
					v.D = math.Inf(1)
				} else if q == "-inf" {
					v.D = math.Inf(-1)
				} else {
					v.D = math.NaN()
				}
			default:
				return fmt.Errorf("invalid double")
			}
			return nil
		case "bool":
			return json.Unmarshal(x, &v.B)
		case "string":
			return json.Unmarshal(x, &v.S)
		case "list":
			return json.Unmarshal(x, &v.L)
		case "map":
			return json.Unmarshal(x, &v.M)
		}
	}
	return fmt.Errorf("invalid typed value")
}
func native(v TV) any {
	switch v.Kind {
	case "int":
		return v.I
	case "double":
		return v.D
	case "bool":
		return v.B
	case "string":
		return v.S
	case "list":
		a := make([]any, len(v.L))
		for i, x := range v.L {
			a[i] = native(x)
		}
		return a
	case "map":
		m := map[string]any{}
		for k, x := range v.M {
			m[k] = native(x)
		}
		return m
	}
	return nil
}
func typed(v ref.Val) (TV, error) {
	switch v.Type().TypeName() {
	case "int":
		return TV{Kind: "int", I: int64(v.(types.Int))}, nil
	case "double":
		return TV{Kind: "double", D: float64(v.(types.Double))}, nil
	case "bool":
		return TV{Kind: "bool", B: bool(v.(types.Bool))}, nil
	case "string":
		return TV{Kind: "string", S: string(v.(types.String))}, nil
	case "list":
		l, ok := v.(traits.Lister)
		if !ok {
			return TV{}, fmt.Errorf("not list")
		}
		out := TV{Kind: "list", L: []TV{}}
		it := l.Iterator()
		for it.HasNext() == types.True {
			q, e := typed(it.Next())
			if e != nil {
				return TV{}, e
			}
			out.L = append(out.L, q)
		}
		return out, nil
	default:
		return TV{}, fmt.Errorf("unsupported CEL result %s", v.Type().TypeName())
	}
}

type Fact struct {
	Rel       string `json:"rel"`
	Args      []TV   `json:"args"`
	ValidFrom *int64 `json:"validFrom,omitempty"`
	ValidTo   *int64 `json:"validTo,omitempty"`
}
type Dump struct {
	Cel  string          `json:"cel"`
	Expr json.RawMessage `json:"expr"`
	Env  struct {
		Variables []struct {
			Name string `json:"name"`
			Type string `json:"type"`
		} `json:"variables"`
	} `json:"env"`
	Activation map[string]TV   `json:"activation"`
	Facts      []Fact          `json:"facts"`
	Now        *int64          `json:"now"`
	Visited    []string        `json:"visited"`
	Result     json.RawMessage `json:"result"`
}

func isErrorResult(raw json.RawMessage) bool {
	var x map[string]json.RawMessage
	_ = json.Unmarshal(raw, &x)
	_, ok := x["error"]
	return ok
}
func tvEq(a, b TV) bool {
	if a.Kind != b.Kind {
		return false
	}
	switch a.Kind {
	case "int":
		return a.I == b.I
	case "double":
		return (math.IsNaN(a.D) && math.IsNaN(b.D)) || a.D == b.D
	case "bool":
		return a.B == b.B
	case "string":
		return a.S == b.S
	case "list":
		if len(a.L) != len(b.L) {
			return false
		}
		for i := range a.L {
			if !tvEq(a.L[i], b.L[i]) {
				return false
			}
		}
		return true
	}
	return false
}
func argsVal(v ref.Val) ([]TV, bool) {
	l, ok := v.(traits.Lister)
	if !ok {
		return nil, false
	}
	out := []TV{}
	it := l.Iterator()
	for it.HasNext() == types.True {
		q, e := typed(it.Next())
		if e != nil {
			return nil, false
		}
		out = append(out, q)
	}
	return out, true
}
func host(d *Dump, name string, args []ref.Val) ref.Val {
	if name == "now" {
		if d.Now == nil {
			return types.NewErr("now unavailable")
		}
		return types.Int(*d.Now)
	}
	if name == "visited" {
		id, ok := args[0].(types.String)
		if !ok {
			return types.NewErr("invalid visited")
		}
		for _, x := range d.Visited {
			if x == string(id) {
				return types.True
			}
		}
		return types.False
	}
	rel, ok := args[0].(types.String)
	if !ok {
		return types.NewErr("invalid relation")
	}
	av, ok := argsVal(args[1])
	if !ok {
		return types.NewErr("invalid arguments")
	}
	match := func(f Fact) bool {
		if f.Rel != string(rel) || len(f.Args) != len(av) {
			return false
		}
		for i := range av {
			if av[i].Kind == "string" && av[i].S == "_" {
				continue
			}
			if !tvEq(f.Args[i], av[i]) {
				return false
			}
		}
		return true
	}
	ms := []Fact{}
	for _, f := range d.Facts {
		if match(f) {
			ms = append(ms, f)
		}
	}
	switch name {
	case "holds":
		return types.Bool(len(ms) > 0)
	case "count":
		return types.Int(len(ms))
	case "countDistinct":
		col, ok := args[2].(types.Int)
		if !ok || int(col) < 0 || int(col) >= len(av) || av[col].Kind != "string" || av[col].S != "_" {
			return types.NewErr("invalid distinct column")
		}
		seen := []TV{}
		for _, f := range ms {
			found := false
			for _, x := range seen {
				if tvEq(x, f.Args[col]) {
					found = true
					break
				}
			}
			if !found {
				seen = append(seen, f.Args[col])
			}
		}
		return types.Int(len(seen))
	case "validAt":
		t, ok := args[2].(types.Int)
		if !ok {
			return types.NewErr("invalid time")
		}
		for _, f := range ms {
			from := int64(0)
			if f.ValidFrom != nil {
				from = *f.ValidFrom
			}
			if from <= int64(t) && (f.ValidTo == nil || int64(t) < *f.ValidTo) {
				return types.True
			}
		}
		return types.False
	}
	return types.NewErr("unknown function")
}
func eval(d *Dump) (TV, bool) {
	opts := []cel.EnvOption{}
	for _, v := range d.Env.Variables {
		opts = append(opts, cel.Variable(v.Name, cel.MapType(cel.StringType, cel.DynType)))
	}
	list := cel.ListType(cel.DynType)
	opts = append(opts, cel.Function("holds", cel.Overload("holds_string_list_dyn", []*cel.Type{cel.StringType, list}, cel.BoolType)), cel.Function("count", cel.Overload("count_string_list_dyn", []*cel.Type{cel.StringType, list}, cel.IntType)), cel.Function("countDistinct", cel.Overload("countDistinct_string_list_dyn_int", []*cel.Type{cel.StringType, list, cel.IntType}, cel.IntType)), cel.Function("validAt", cel.Overload("validAt_string_list_dyn_int", []*cel.Type{cel.StringType, list, cel.IntType}, cel.BoolType)), cel.Function("now", cel.Overload("now_int", []*cel.Type{}, cel.IntType)), cel.Function("visited", cel.Overload("visited_string", []*cel.Type{cel.StringType}, cel.BoolType)))
	e, er := cel.NewEnv(opts...)
	if er != nil {
		return TV{}, false
	}
	ast, iss := e.Parse(d.Cel)
	if iss.Err() != nil {
		return TV{}, false
	}
	checked, iss := e.Check(ast)
	if iss.Err() != nil {
		return TV{}, false
	}
	overload := func(id, name string) *functions.Overload {
		return &functions.Overload{Operator: id, Function: func(a ...ref.Val) ref.Val { return host(d, name, a) }}
	}
	p, er := e.Program(checked, cel.Functions(overload("holds_string_list_dyn", "holds"), overload("count_string_list_dyn", "count"), overload("countDistinct_string_list_dyn_int", "countDistinct"), overload("validAt_string_list_dyn_int", "validAt"), overload("now_int", "now"), overload("visited_string", "visited")))
	if er != nil {
		return TV{}, false
	}
	act := map[string]any{}
	for k, v := range d.Activation {
		act[k] = native(v)
	}
	out, _, er := p.Eval(act)
	if er != nil || out.Type().TypeName() == "error" || out.Type().TypeName() == "unknown" {
		return TV{}, false
	}
	q, er := typed(out)
	return q, er == nil
}
func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "usage: harness <dump.jsonl>...")
		os.Exit(2)
	}
	bad, line := 0, 0
	var env Dump
	for _, file := range os.Args[1:] {
		f, e := os.Open(file)
		if e != nil {
			panic(e)
		}
		sc := bufio.NewScanner(f)
		sc.Buffer(make([]byte, 64*1024), 16*1024*1024)
		for sc.Scan() {
			s := sc.Text()
			if strings.TrimSpace(s) == "" {
				continue
			}
			var raw map[string]json.RawMessage
			if json.Unmarshal([]byte(s), &raw) != nil {
				bad++
				continue
			}
			if string(raw["kind"]) == `"env"` {
				var header Dump
				if json.Unmarshal([]byte(s), &header) != nil {
					bad++
					continue
				}
				env.Env = header.Env
				continue
			}
			line++
			var d Dump
			if json.Unmarshal([]byte(s), &d) != nil {
				bad++
				continue
			}
			d.Env = env.Env
			wantErr := isErrorResult(d.Result)
			got, ok := eval(&d)
			if wantErr != (!ok) {
				fmt.Fprintf(os.Stderr, "line %d cel %q: expected error=%v got error=%v\n", line, d.Cel, wantErr, !ok)
				bad++
			} else if !wantErr {
				var want TV
				if json.Unmarshal(d.Result, &want) != nil || !tvEq(want, got) {
					fmt.Fprintf(os.Stderr, "line %d cel %q: result mismatch\n", line, d.Cel)
					bad++
				}
			}
		}
		if e := sc.Err(); e != nil {
			panic(e)
		}
		_ = f.Close()
	}
	if bad > 0 {
		os.Exit(1)
	}
	fmt.Printf("ok: %d condition evaluations\n", line)
}
