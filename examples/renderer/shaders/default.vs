#version 130

precision highp float;

uniform mat4 modelView;
uniform mat4 projection;
uniform mat4 viewMatrix;
uniform mat3 normalMatrix;

attribute vec3 position;
attribute vec3 normal;

varying vec3 vViewPosition;
varying vec3 vNormal;

void main() {
  vNormal = normalMatrix * normal;
  
  vec4 adjustedPosition = vec4(position, 1.0);
  vec4 mvPosition = modelView * adjustedPosition;
  
  gl_Position = projection * mvPosition;
  vViewPosition = -mvPosition.xyz;
}
