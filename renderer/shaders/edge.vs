#version 100

precision highp float;

attribute vec3 position;
attribute vec3 color;

uniform mat4 projection;
uniform mat4 modelView;

/* not used */
uniform vec4 lightColor;
uniform vec4 lightDirection;

varying vec4 vColor;

void main(void) {
     vec4 pos4 = vec4(position, 1.0);
     gl_Position = projection * modelView * pos4;
     vColor = vec4(color, 1.0);
}
