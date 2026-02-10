package client

import "time"

// BoxOptions configures a new box.
// While exported, it is recommended to use functional options with CreateBox.
type BoxOptions struct {
	// Image is the OCI image to use (e.g., "alpine:latest").
	Image string `json:"image"`

	// CPUs is the number of virtual CPUs (default: 1).
	CPUs int `json:"cpus,omitempty"`

	// MemoryMB is the memory limit in megabytes (default: 512).
	MemoryMB int `json:"memory_mb,omitempty"`

	// Env is a map of environment variables.
	Env map[string]string `json:"env,omitempty"`

	// WorkingDir is the working directory inside the container.
	WorkingDir string `json:"working_dir,omitempty"`
}

// Option is a functional option for configuring a box.
type Option func(*BoxOptions)

// WithImage sets the OCI image to use.
func WithImage(image string) Option {
	return func(o *BoxOptions) {
		o.Image = image
	}
}

// WithCPUs sets the number of virtual CPUs.
func WithCPUs(cpus int) Option {
	return func(o *BoxOptions) {
		o.CPUs = cpus
	}
}

// WithMemoryMB sets the memory limit in megabytes.
func WithMemoryMB(memoryMB int) Option {
	return func(o *BoxOptions) {
		o.MemoryMB = memoryMB
	}
}

// WithEnv sets an environment variable.
func WithEnv(key, value string) Option {
	return func(o *BoxOptions) {
		if o.Env == nil {
			o.Env = make(map[string]string)
		}
		o.Env[key] = value
	}
}

// WithWorkingDir sets the working directory inside the container.
func WithWorkingDir(dir string) Option {
	return func(o *BoxOptions) {
		o.WorkingDir = dir
	}
}

// BoxInfo contains information about a box.
type BoxInfo struct {
	ID        string    `json:"id"`
	Name      string    `json:"name,omitempty"`
	Image     string    `json:"image"`
	State     string    `json:"state"`
	CreatedAt time.Time `json:"created_at"`
}

// BoxState represents the state of a box.
type BoxState string

const (
	BoxStateConfigured BoxState = "configured"
	BoxStateRunning    BoxState = "running"
	BoxStateStopped    BoxState = "stopped"
	BoxStateError      BoxState = "error"
)